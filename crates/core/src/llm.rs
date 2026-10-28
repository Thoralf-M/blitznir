//! The OpenAI-compatible client: model lists and streamed chat completions,
//! with the reasoning retry and the answer unwrap. The only async code.
//!
//! Streaming is SSE `data:` lines. Dropping the response closes the socket,
//! which is what makes the server stop generating, so cancel = drop.

pub mod release;
mod sse;
pub mod vision;

use crate::config::{Config, Endpoint};
use crate::store::State;
use crate::{Error, Result, tasks};
use futures_util::StreamExt;
use regex::Regex;
use serde_json::Value;
use sse::{Attempt, Line, parse_line, strip_wrappers};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

/// Never ask for more than this in one request; also the retry's budget.
pub const MAX_TOKENS_CEILING: usize = 8192;

/// Called with the unwrapped answer so far.
pub type OnDelta<'a> = dyn FnMut(&str) + Send + 'a;

/// What an answer goes through as it streams: [`strip_wrappers`] for a
/// text to paste, nothing for a chat's markdown.
type Unwrap = fn(&str) -> String;

/// Cancels a running request; clones share the state.
#[derive(Clone)]
pub struct CancellationToken(Arc<watch::Sender<bool>>);

impl Default for CancellationToken {
    fn default() -> Self {
        CancellationToken(Arc::new(watch::channel(false).0))
    }
}

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }

    pub async fn cancelled(&self) {
        let _ = self.0.subscribe().wait_for(|c| *c).await;
    }
}

/// `chat_template_kwargs` is local servers' extension (llama.cpp, LM Studio,
/// oMLX); OpenAI-style cloud APIs refuse a request with a field they do not know.
fn is_local(url: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(url) else {
        return false;
    };
    let host = u.host_str().unwrap_or_default().trim_matches(['[', ']']);
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// `body` as the endpoint at `url` accepts it.
fn portable(url: &str, mut body: Value) -> Value {
    if !is_local(url)
        && let Some(map) = body.as_object_mut()
    {
        map.remove("chat_template_kwargs");
    }
    body
}

pub struct Client {
    http: reqwest::Client,
}

impl Client {
    pub fn new() -> Result<Client> {
        // a read timeout, not a total one: a long answer may stream for minutes
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(180))
            .build()?;
        Ok(Client { http })
    }

    fn request(
        &self,
        method: reqwest::Method,
        ep: &Endpoint,
        path: &str,
    ) -> reqwest::RequestBuilder {
        let req = self
            .http
            .request(method, format!("{}/{path}", ep.url.trim_end_matches('/')));
        let key = ep.key.trim();
        if key.is_empty() {
            req
        } else {
            req.bearer_auth(key)
        }
    }

    /// The chat models an endpoint offers: its `models` list as written, or
    /// the server's list without drafters and helpers.
    pub async fn models(&self, ep: &Endpoint, hide: &Regex) -> Result<Vec<String>> {
        if !ep.models.is_empty() {
            return Ok(ep.models.clone());
        }
        let mut names = self.fetch_models(ep).await?;
        names.retain(|n| tasks::is_chat_model(hide, n));
        names.sort();
        names.dedup();
        Ok(names)
    }

    /// The model picker's rows: every endpoint asked at once, so a dead one
    /// costs its 5 s timeout and not the others'.
    pub async fn list_models(&self, cfg: &Config) -> Vec<Listing> {
        // validate() refused a broken pattern; hide nothing rather than panic
        let never = || Regex::new("$^").expect("constant regex");
        let hide = tasks::hide_regex(&cfg.llm.hide_models).unwrap_or_else(|_| never());
        let eye = tasks::hide_regex(&cfg.llm.vision_models).unwrap_or_else(|_| never());
        let asks = cfg.endpoints.iter().map(|ep| {
            let (hide, eye) = (&hide, &eye);
            async move {
                let models = self.models(ep, hide).await;
                // after the list, not beside it: a dead server costs one timeout
                let types = match &models {
                    Ok(_) => self.model_types(ep).await,
                    Err(_) => vision::Types::new(),
                };
                let vision = models.iter().flatten();
                let vision = vision.filter(|m| vision::accepts(ep, &types, eye, m));
                Listing {
                    endpoint: ep.name.clone(),
                    vision: vision.cloned().collect(),
                    models,
                }
            }
        });
        futures_util::future::join_all(asks).await
    }

    async fn fetch_models(&self, ep: &Endpoint) -> Result<Vec<String>> {
        let req = self
            .request(reqwest::Method::GET, ep, "models")
            .timeout(Duration::from_secs(5));
        let resp = check(req.send().await.map_err(|e| unreachable(ep, e))?).await?;
        let data: Value = resp.json().await?;
        // OpenAI's {"data":[{"id"}]} or the Ollama-style {"models":[{"name"}]}
        let list = data["data"]
            .as_array()
            .or_else(|| data["models"].as_array())
            .cloned()
            .unwrap_or_default();
        Ok(list
            .iter()
            .filter_map(|e| {
                e.as_str()
                    .or_else(|| ["id", "model", "name"].iter().find_map(|k| e[k].as_str()))
                    .map(str::to_owned)
            })
            .collect())
    }

    /// A text to paste, streamed and unwrapped ([`strip_wrappers`]).
    pub async fn stream(
        &self,
        ep: &Endpoint,
        body: Value,
        on_delta: &mut OnDelta<'_>,
        cancel: &CancellationToken,
    ) -> Result<String> {
        self.answer(ep, body, (on_delta, strip_wrappers), cancel)
            .await
    }

    /// [`Client::stream`] for a chat: its markdown as written, a code block
    /// that opens or closes the answer included.
    pub async fn chat(
        &self,
        ep: &Endpoint,
        body: Value,
        on_delta: &mut OnDelta<'_>,
        cancel: &CancellationToken,
    ) -> Result<String> {
        self.answer(ep, body, (on_delta, str::to_owned), cancel)
            .await
    }

    /// Stream one chat completion; only a whole answer is Ok. Retries ONCE
    /// with the ceiling as budget when the tokens ran out (cut off, or
    /// reasoned without answering): retrying beats losing the user's rewrite.
    async fn answer(
        &self,
        ep: &Endpoint,
        body: Value,
        (on_delta, unwrap): (&mut OnDelta<'_>, Unwrap),
        cancel: &CancellationToken,
    ) -> Result<String> {
        let mut body = portable(&ep.url, body);
        body["stream"] = true.into();
        let mut a = self.attempt(ep, &body, (on_delta, unwrap), cancel).await?;
        let mut budget = body["max_tokens"].as_u64().unwrap_or(0) as usize;
        if a.starved() && budget < MAX_TOKENS_CEILING {
            log::warn!(
                "out of tokens at max_tokens={budget} ({} chars of reasoning) — retrying with max_tokens={MAX_TOKENS_CEILING}",
                a.reasoning
            );
            budget = MAX_TOKENS_CEILING;
            body["max_tokens"] = budget.into();
            a = self.attempt(ep, &body, (on_delta, unwrap), cancel).await?;
        }
        let asked = body["chat_template_kwargs"]["enable_thinking"] == true;
        a.answer(budget, asked)
    }

    /// One request, read to its end.
    async fn attempt(
        &self,
        ep: &Endpoint,
        body: &Value,
        (on_delta, unwrap): (&mut OnDelta<'_>, Unwrap),
        cancel: &CancellationToken,
    ) -> Result<Attempt> {
        let send = self
            .request(reqwest::Method::POST, ep, "chat/completions")
            .json(body)
            .send();
        let resp = tokio::select! {
            () = cancel.cancelled() => return Err(Error::Cancelled),
            r = send => check(r.map_err(|e| unreachable(ep, e))?).await?,
        };
        let mut bytes = resp.bytes_stream();
        let (mut buf, mut content, mut reasoning) = (Vec::new(), String::new(), 0);
        let mut finish: Option<String> = None;
        let ended = 'read: loop {
            let chunk = tokio::select! {
                () = cancel.cancelled() => return Err(Error::Cancelled), // drops the connection
                c = bytes.next() => c,
            };
            let Some(chunk) = chunk else {
                break sse::ended(finish.as_deref(), false);
            };
            buf.extend_from_slice(&chunk?);
            while let Some(nl) = buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buf.drain(..=nl).collect();
                match parse_line(&String::from_utf8_lossy(&line)) {
                    Line::Skip => {}
                    Line::Done => break 'read sse::ended(finish.as_deref(), true),
                    Line::Failed(msg) => break 'read sse::Ended::Broken(msg),
                    Line::Delta {
                        content: c,
                        reasoning: r,
                        finish: f,
                    } => {
                        content.push_str(&c);
                        reasoning += r;
                        finish = f.or(finish);
                        if !c.is_empty() {
                            on_delta(&unwrap(&content));
                        }
                    }
                }
            }
        };
        Ok(Attempt {
            content: unwrap(&content),
            reasoning,
            ended,
        })
    }

    /// Translate: ask which language `text` is in, then translate in that direction.
    /// Images are read by the model; with no text there is nothing to ask.
    #[allow(clippy::too_many_arguments)] // one request's inputs, each distinct
    pub async fn translate(
        &self,
        ep: &Endpoint,
        cfg: &Config,
        model: &str,
        text: &str,
        images: &[String],
        on_delta: &mut OnDelta<'_>,
        cancel: &CancellationToken,
    ) -> Result<String> {
        let instruction = self.direction(ep, cfg, model, text, cancel).await?;
        self.stream(
            ep,
            tasks::rewrite_body(cfg, model, text, "", Some(&instruction), images),
            on_delta,
            cancel,
        )
        .await
    }

    /// Translate's instruction for `text`, in the direction its language check says.
    pub async fn direction(
        &self,
        ep: &Endpoint,
        cfg: &Config,
        model: &str,
        text: &str,
        cancel: &CancellationToken,
    ) -> Result<String> {
        let home = &cfg.translate.home;
        let answer = if text.trim().is_empty() {
            String::new()
        } else {
            self.lang_check(ep, model, home, text, cancel).await?
        };
        let from_home = tasks::is_home(&answer, home, text);
        Ok(tasks::translate_instruction(from_home, &cfg.translate))
    }

    /// The one-word answer, or "" when the check failed (then a guess decides).
    async fn lang_check(
        &self,
        ep: &Endpoint,
        model: &str,
        home: &str,
        text: &str,
        cancel: &CancellationToken,
    ) -> Result<String> {
        let body = tasks::lang_check_body(model, home, text);
        match self.stream(ep, body, &mut |_| {}, cancel).await {
            Err(Error::Cancelled) => Err(Error::Cancelled),
            Err(e) => {
                log::warn!("language check failed ({e}) — guessing");
                Ok(String::new())
            }
            Ok(a) => Ok(a),
        }
    }
}

/// One endpoint's answer for the model picker.
pub struct Listing {
    pub endpoint: String,
    pub models: Result<Vec<String>>,
    /// The ids among `models` that accept images ([`vision::accepts`]).
    pub vision: Vec<String>,
}

/// The endpoint and model picked in the panel, while config.toml still has it.
pub fn target<'a>(cfg: &'a Config, state: &'a State) -> Result<(&'a Endpoint, &'a str)> {
    if state.model.is_empty() {
        return Err(Error::Llm(
            "No model picked: pick one at the top right".into(),
        ));
    }
    let ep = cfg
        .endpoints
        .iter()
        .find(|e| e.name == state.endpoint)
        .ok_or_else(|| {
            Error::Llm(format!(
                "Endpoint {:?} is not in config.toml: pick a model at the top right",
                state.endpoint
            ))
        })?;
    Ok((ep, &state.model))
}

fn unreachable(ep: &Endpoint, e: reqwest::Error) -> Error {
    if e.is_connect() {
        Error::Llm(format!(
            "cannot reach {} at {} — is the server running?",
            ep.name, ep.url
        ))
    } else {
        Error::Http(e)
    }
}

/// A non-2xx response becomes the server's own error message.
async fn check(resp: reqwest::Response) -> Result<reqwest::Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let body = resp.text().await.unwrap_or_default();
    let json: Value = serde_json::from_str(&body).unwrap_or_default();
    let msg = sse::error_message(&json).unwrap_or_else(|| body.chars().take(300).collect());
    Err(Error::Llm(format!("{status}: {msg}")))
}

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
