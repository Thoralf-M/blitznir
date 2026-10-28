//! Whether a model accepts images, for the picker's eye. In order: the
//! endpoint's `vision = true|false`, else what the server says where it
//! describes its models next to /v1 (`/api/v0/models`, `"type": "vlm"`; LM
//! Studio does, any server may), else `[llm] vision_models` on the id.

use super::Client;
use crate::config::{Config, Endpoint};
use crate::tasks;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

/// Model id → the server says it takes images.
pub type Types = HashMap<String, bool>;

/// Where a server that describes its models keeps the list: beside /v1.
pub fn types_url(url: &str) -> String {
    let base = url.trim_end_matches('/');
    let base = base.strip_suffix("/v1").unwrap_or(base);
    format!("{base}/api/v0/models")
}

/// `/api/v0/models`'s answer, `{"data": [{"id", "type"}]}`, as [`Types`].
pub fn parse_types(data: &Value) -> Types {
    data["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| Some((m["id"].as_str()?.to_owned(), m["type"].as_str()? == "vlm")))
        .collect()
}

/// The eye for model `id` on `ep`; `pattern` is `vision_models`, compiled.
pub fn accepts(ep: &Endpoint, types: &Types, pattern: &Regex, id: &str) -> bool {
    ep.vision
        .or_else(|| types.get(id).copied())
        .unwrap_or_else(|| pattern.is_match(id))
}

/// The eye for a model no listing describes (the picked one, its server
/// down): the endpoint's override, else the pattern.
pub fn unlisted(cfg: &Config, endpoint: &str, id: &str) -> bool {
    let ep = cfg.endpoints.iter().find(|e| e.name == endpoint);
    let pattern = || tasks::hide_regex(&cfg.llm.vision_models).is_ok_and(|re| re.is_match(id));
    ep.and_then(|e| e.vision).unwrap_or_else(pattern)
}

impl Client {
    /// What `ep` says about its models' types; a server that does not say
    /// (any error) is left to the pattern. Not asked when `ep` overrides.
    pub(super) async fn model_types(&self, ep: &Endpoint) -> Types {
        if ep.vision.is_some() {
            return Types::new();
        }
        let mut req = self
            .http
            .get(types_url(&ep.url))
            .timeout(Duration::from_secs(5));
        if !ep.key.trim().is_empty() {
            req = req.bearer_auth(ep.key.trim());
        }
        let data = match req.send().await.and_then(|r| r.error_for_status()) {
            Ok(r) => r.json::<Value>().await.unwrap_or_default(),
            Err(_) => Value::Null,
        };
        parse_types(&data)
    }
}

#[cfg(test)]
mod tests;
