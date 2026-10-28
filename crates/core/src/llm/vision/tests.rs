use super::*;
use crate::config::VISION_MODELS;
use crate::llm::mock::{Reply, dead_endpoint, mock};
use crate::tasks::hide_regex;
use serde_json::json;

#[test]
fn the_override_beats_the_server_which_beats_the_pattern() {
    let re = hide_regex(VISION_MODELS).unwrap();
    let mut ep = dead_endpoint("x");
    let types = parse_types(&json!({"data": [
        {"id": "qwen3-vl-8b", "type": "llm"},
        {"id": "plain-7b", "type": "vlm"},
        {"id": "no-type"},
    ]}));
    assert_eq!(types.len(), 2, "a row without a type says nothing");
    assert!(
        !accepts(&ep, &types, &re, "qwen3-vl-8b"),
        "the server knows best"
    );
    assert!(accepts(&ep, &types, &re, "plain-7b"));
    assert!(accepts(
        &ep,
        &types,
        &re,
        "mlx-community/Qwen2.5-VL-7B-Instruct-4bit"
    ));
    assert!(accepts(&ep, &types, &re, "google/gemma-4-e4b-it"));
    assert!(!accepts(
        &ep,
        &types,
        &re,
        "mlx-community/Qwen3-Coder-30B-A3B-Instruct-4bit"
    ));
    ep.vision = Some(false);
    assert!(!accepts(&ep, &types, &re, "plain-7b"));
    ep.vision = Some(true);
    assert!(accepts(&ep, &types, &re, "no-type"));
    assert_eq!(
        types_url("http://h:1234/v1/"),
        "http://h:1234/api/v0/models"
    );
    assert_eq!(types_url("http://h:8000"), "http://h:8000/api/v0/models");
    let mut cfg = Config::default();
    assert!(unlisted(&cfg, "LM Studio", "pixtral-12b"));
    assert!(!unlisted(&cfg, "LM Studio", "qwen3-8b"));
    cfg.endpoints[0].vision = Some(true);
    assert!(unlisted(&cfg, &cfg.endpoints[0].name.clone(), "qwen3-8b"));
}

#[tokio::test]
async fn every_endpoint_is_listed_with_its_eyes_and_a_dead_one_says_why() {
    let list = json!({"data": [{"id": "b"}, {"id": "a-mtp"}, {"id": "a"}, {"id": "llava-x"}]});
    let types = json!({"data": [{"id": "b", "type": "vlm"}, {"id": "a", "type": "llm"}]});
    let replies = vec![
        Reply::Plain(200, list.to_string()),
        Reply::Plain(200, types.to_string()),
    ];
    let m = mock(replies, "");
    let cfg = Config {
        endpoints: vec![m.ep.clone(), dead_endpoint("oMLX")],
        ..Config::default()
    };
    let lists = Client::new().unwrap().list_models(&cfg).await;
    assert_eq!(lists[0].endpoint, "mock");
    assert_eq!(lists[0].models.as_ref().unwrap(), &["a", "b", "llava-x"]);
    assert_eq!(lists[0].vision, ["b", "llava-x"]);
    assert_eq!(lists[1].endpoint, "oMLX");
    let err = lists[1].models.as_ref().unwrap_err().to_string();
    assert!(err.starts_with("cannot reach oMLX"), "{err}");
    m.server.join().unwrap();
    assert!(
        m.requests.lock().unwrap()[1]
            .0
            .starts_with("GET /api/v0/models ")
    );
}
