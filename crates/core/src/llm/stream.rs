//! One streamed chat completion: the request, its SSE read to the end, and
//! the retries of an answer that ran out of tokens.

use super::looping::Watch;
use super::sse::{self, Attempt, Ended, Line, parse_line};
use super::{CancellationToken, Client, OnDelta, Unwrap, check, portable, unreachable};
use crate::config::Endpoint;
use crate::store::Unthought;
use crate::{Error, Result};
use futures_util::StreamExt;
use serde_json::Value;

/// A whole answer, and the reasoning behind it.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Answer {
    pub text: String,
    /// The reasoning of the last attempt that had any; "" when none did.
    pub thinking: String,
    /// It was asked to think and answered without ([`sse::escalate`]):
    /// `thinking` is the trace that ran out or looped.
    pub unthought: Unthought,
}

/// An answer's callbacks: its text so far, its reasoning so far; and what
/// the text goes through.
type Calls<'c, 'a, 'b> = (&'c mut OnDelta<'a>, &'c mut OnDelta<'b>, Unwrap);

impl Client {
    /// Stream one chat completion; only a whole answer is Ok. When the tokens
    /// ran out (cut off, reasoned without answering, or looped) it retries as
    /// [`sse::escalate`] says: retrying beats losing the user's rewrite.
    pub(super) async fn answer(
        &self,
        ep: &Endpoint,
        body: Value,
        (on_delta, on_think, unwrap): Calls<'_, '_, '_>,
        cancel: &CancellationToken,
    ) -> Result<Answer> {
        let mut body = portable(&ep.url, body);
        body["stream"] = true.into();
        let thinks = |b: &Value| b["chat_template_kwargs"]["enable_thinking"] == true;
        let asked = thinks(&body);
        let mut a = self
            .attempt(ep, &body, (on_delta, on_think, unwrap), cancel)
            .await?;
        let mut thinking = a.reasoning.clone();
        let mut looped = a.ended == Ended::Looped;
        while a.starved() && sse::escalate(&mut body, &a.reasoning, a.ended == Ended::Looped) {
            a = self
                .attempt(ep, &body, (on_delta, on_think, unwrap), cancel)
                .await?;
            looped |= a.ended == Ended::Looped;
            if !a.reasoning.is_empty() {
                thinking.clone_from(&a.reasoning);
            }
        }
        let budget = body["max_tokens"].as_u64().unwrap_or(0) as usize;
        // a model that thinks anyway, asked not to, answered with its own trace
        let unthought = match asked && !thinks(&body) && a.reasoning.is_empty() {
            false => Unthought::No,
            true if looped => Unthought::Looped,
            true => Unthought::RanOut,
        };
        Ok(Answer {
            text: a.answer(budget, asked)?,
            thinking,
            unthought,
        })
    }

    /// One request, read to its end, or dropped once its reasoning loops.
    async fn attempt(
        &self,
        ep: &Endpoint,
        body: &Value,
        (on_delta, on_think, unwrap): Calls<'_, '_, '_>,
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
        let (mut buf, mut content, mut reasoning) = (Vec::new(), String::new(), String::new());
        let mut finish: Option<String> = None;
        let mut watch = Watch::default();
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
                        reasoning.push_str(&r);
                        finish = f.or(finish);
                        if !c.is_empty() {
                            on_delta(&unwrap(&content));
                        }
                        if !r.is_empty() {
                            on_think(&reasoning);
                            if watch.looped(&reasoning, &r) {
                                let n = watch.chars();
                                log::warn!(
                                    "the reasoning loops: caught at {n} chars — dropping the request"
                                );
                                break 'read Ended::Looped; // the connection goes with it
                            }
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
}
