use super::{
    mock::{Mock, Reply, dead_endpoint, mock},
    *,
};
use crate::{
    store::{State, Unthought},
    tasks::tests::{block, prompts_md, vectors},
};
use serde_json::json;

mod fallback;

fn data(v: &Value) -> String {
    format!("data: {v}\n")
}

fn content(text: &str) -> String {
    data(&json!({"choices": [{"delta": {"content": text}}]}))
}

fn thinking(key: &str, text: &str) -> String {
    data(&json!({"choices": [{"delta": {key: text}}]}))
}

fn done() -> String {
    "data: [DONE]\n".into()
}

fn finish(reason: &str) -> String {
    data(&json!({"choices": [{"delta": {}, "finish_reason": reason}]}))
}

fn body(max_tokens: usize) -> Value {
    json!({"model": "m", "messages": [], "max_tokens": max_tokens, "chat_template_kwargs": {"enable_thinking": false}})
}

async fn ask(m: &Mock, max_tokens: usize) -> (Result<String>, Vec<String>) {
    let mut seen = vec![];
    let r = Client::new()
        .unwrap()
        .stream(
            &m.ep,
            body(max_tokens),
            &mut |t| seen.push(t.to_string()),
            &CancellationToken::default(),
        )
        .await;
    (r, seen)
}

#[tokio::test]
async fn deltas_stream_in_order() {
    let lines = vec![
        ": keep-alive\n".into(),
        content("Hel"),
        "data: not json\n".into(),
        content("lo"),
        content(" world"),
        finish("stop"),
        done(),
    ];
    let m = mock(vec![Reply::Sse(lines)], "sk-test");
    let (r, seen) = ask(&m, 100).await;
    assert_eq!(r.unwrap(), "Hello world");
    assert_eq!(seen, ["Hel", "Hello", "Hello world"]);
    m.server.join().unwrap();
    let reqs = m.requests.lock().unwrap();
    let (head, sent) = &reqs[0];
    assert!(head.starts_with("POST /v1/chat/completions "), "{head}");
    assert!(
        head.to_lowercase()
            .contains("authorization: bearer sk-test"),
        "{head}"
    );
    assert_eq!(sent["stream"], true);
    assert_eq!(
        sent["chat_template_kwargs"]["enable_thinking"], false,
        "127.0.0.1 is local"
    );
}

#[tokio::test]
async fn fences_and_quotes_are_unwrapped() {
    let lines = vec![
        content("```text\n\"Hal"),
        content("lo\"\n"),
        content("```"),
        done(),
    ];
    let m = mock(vec![Reply::Sse(lines)], "");
    assert_eq!(ask(&m, 100).await.0.unwrap(), "Hallo");
    assert!(
        !m.requests.lock().unwrap()[0]
            .0
            .to_lowercase()
            .contains("authorization")
    );
}

#[test]
fn a_fence_line_goes_whatever_language_it_names() {
    for (fenced, code) in [
        ("```c++\nint x;\n```", "int x;"),
        ("```c#\nvar x = 1;\n```", "var x = 1;"),
        ("```objective-c\n[x y];\n```", "[x y];"),
        ("```\nplain\n```", "plain"),
    ] {
        assert_eq!(super::sse::strip_wrappers(fenced), code);
    }
}

#[tokio::test]
async fn a_chat_answer_keeps_its_fences() {
    let fenced = "```py\nx = 1\n```\nThat sets x.";
    let m = mock(vec![Reply::Sse(vec![content(fenced), done()])], "");
    let mut seen = vec![];
    let r = Client::new()
        .unwrap()
        .chat(
            &m.ep,
            body(100),
            &mut |t| seen.push(t.to_string()),
            &mut |_| {},
            &CancellationToken::default(),
        )
        .await;
    assert_eq!(r.unwrap().text, fenced);
    assert_eq!(seen, [fenced], "streamed as written too");
}

async fn chat(m: &Mock, b: Value) -> Answer {
    chat_seen(m, b).await.0
}

/// [`chat`], and the reasoning as it streamed.
async fn chat_seen(m: &Mock, b: Value) -> (Answer, Vec<String>) {
    let mut seen = vec![];
    let r = Client::new()
        .unwrap()
        .chat(
            &m.ep,
            b,
            &mut |_| {},
            &mut |t| seen.push(t.to_string()),
            &CancellationToken::default(),
        )
        .await;
    (r.unwrap(), seen)
}

#[tokio::test]
async fn a_chat_answer_keeps_its_thinking_under_either_key() {
    let lines = vec![
        thinking("reasoning_content", "First, "),
        thinking("reasoning", "the sky."),
        content("Blue."),
        done(),
    ];
    let m = mock(vec![Reply::Sse(lines)], "");
    let (a, seen) = chat_seen(&m, body(100)).await;
    let want = ("Blue.", "First, the sky.", Unthought::No);
    assert_eq!((a.text.as_str(), a.thinking.as_str(), a.unthought), want);
    assert_eq!(seen, ["First, ", "First, the sky."], "streamed as it came");
    m.server.join().unwrap();
}

#[test]
fn unwrap_vectors() {
    let md = prompts_md();
    for (input, want) in vectors(&block(&md, "## Unwrap", "~~~~json", 0)) {
        assert_eq!(strip_wrappers(&input), want.as_str().unwrap(), "{input:?}");
    }
}

#[tokio::test]
async fn an_answer_lost_to_reasoning_is_retried_once_at_the_ceiling() {
    let first = vec![thinking("reasoning", "hmm, let me think"), done()];
    let second = vec![
        thinking("reasoning_content", "short"),
        content("ok"),
        done(),
    ];
    let m = mock(vec![Reply::Sse(first), Reply::Sse(second)], "");
    assert_eq!(ask(&m, 2312).await.0.unwrap(), "ok");
    m.server.join().unwrap();
    let budgets: Vec<Value> = m
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|(_, b)| b["max_tokens"].clone())
        .collect();
    assert_eq!(budgets, [json!(2312), json!(MAX_TOKENS_CEILING)]);
}

#[tokio::test]
async fn no_second_retry_and_none_at_the_ceiling() {
    let empty = || Reply::Sse(vec![thinking("reasoning", "thinking…"), done()]);
    let m = mock(vec![empty(), empty()], "");
    let err = ask(&m, 100).await.0.unwrap_err().to_string();
    assert!(err.contains("reasoning (9 chars)"), "{err}");
    m.server.join().unwrap();
    let m = mock(vec![empty()], "");
    assert!(ask(&m, MAX_TOKENS_CEILING).await.0.is_err());
    m.server.join().unwrap();
    assert_eq!(m.requests.lock().unwrap().len(), 1);
    // no content and no reasoning: nothing to retry for
    let m = mock(vec![Reply::Sse(vec![done()])], "");
    assert_eq!(
        ask(&m, 100).await.0.unwrap_err().to_string(),
        "the model returned nothing"
    );
}

#[tokio::test]
async fn a_cut_off_answer_is_retried_once_then_refused() {
    let cut = || Reply::Sse(vec![content("Half an ans"), finish("length"), done()]);
    let m = mock(vec![cut(), cut()], "");
    let err = ask(&m, 100).await.0.unwrap_err().to_string();
    assert_eq!(
        err,
        format!("the answer was cut off at max_tokens={MAX_TOKENS_CEILING}")
    );
    m.server.join().unwrap();
    assert_eq!(m.requests.lock().unwrap().len(), 2);
    let whole = Reply::Sse(vec![content("A whole answer."), finish("stop")]);
    let m = mock(vec![cut(), whole], "");
    let (r, seen) = ask(&m, 100).await;
    assert_eq!(r.unwrap(), "A whole answer.", "stop, no [DONE]: whole");
    assert_eq!(seen, ["Half an ans", "A whole answer."]);
}

#[tokio::test]
async fn an_error_or_an_early_end_in_the_stream_is_no_answer() {
    let oom = data(&json!({"error": {"message": "out of memory"}}));
    let m = mock(vec![Reply::Sse(vec![content("partial"), oom])], "");
    let err = ask(&m, 100).await.0.unwrap_err().to_string();
    assert_eq!(err, "out of memory");
    m.server.join().unwrap();
    assert_eq!(m.requests.lock().unwrap().len(), 1, "no retry");
    let m = mock(vec![Reply::Sse(vec![content("partial")])], "");
    let err = ask(&m, 100).await.0.unwrap_err().to_string();
    assert!(err.contains("ended before the answer"), "{err}");
}

#[tokio::test]
async fn cancel_drops_the_connection() {
    let m = mock(vec![Reply::Hold(vec![content("first words")])], "");
    let cancel = CancellationToken::default();
    let c = cancel.clone();
    let client = Client::new().unwrap(); // kept alive: the close must come from the cancel
    let r = client
        .stream(&m.ep, body(100), &mut |_| c.cancel(), &cancel)
        .await;
    assert!(matches!(r, Err(Error::Cancelled)), "{r:?}");
    // wait without blocking the runtime: the connection task does the close
    let mut hung_up = None;
    for _ in 0..60 {
        hung_up = m.hung_up.try_recv().ok();
        if hung_up.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(hung_up, Some(true), "the server saw the socket close");
    drop(client);
}

#[test]
fn enable_thinking_goes_to_local_hosts_only() {
    for url in [
        "http://127.0.0.1:1234/v1",
        "http://localhost:8000/v1",
        "http://[::1]:1234/v1",
    ] {
        assert!(
            portable(url, body(1)).get("chat_template_kwargs").is_some(),
            "{url}"
        );
    }
    for url in [
        "https://api.openai.com/v1",
        "http://192.168.1.5:1234/v1",
        "http://localhost.example.com/v1",
        "nonsense",
    ] {
        assert!(
            portable(url, body(1)).get("chat_template_kwargs").is_none(),
            "{url}"
        );
    }
}

#[tokio::test]
async fn models_come_sorted_without_drafters() {
    let openai = json!({"data": [{"id": "qwen-b"}, {"id": "gemma-a"}, {"id": "mtp-gemma"}, {"id": "gemma-a"}]}).to_string();
    let ollama = json!({"models": [{"name": "llama3"}, "phi4"]}).to_string();
    let m = mock(
        vec![Reply::Plain(200, openai), Reply::Plain(200, ollama)],
        "",
    );
    let hide = tasks::hide_regex(crate::config::HIDE_MODELS).unwrap();
    let client = Client::new().unwrap();
    assert_eq!(
        client.models(&m.ep, &hide).await.unwrap(),
        ["gemma-a", "qwen-b"]
    );
    assert_eq!(
        client.models(&m.ep, &hide).await.unwrap(),
        ["llama3", "phi4"]
    );
    let listed = Endpoint {
        models: vec!["mine".into()],
        ..m.ep.clone()
    };
    assert_eq!(client.models(&listed, &hide).await.unwrap(), ["mine"]); // no request
    m.server.join().unwrap();
    assert!(
        m.requests.lock().unwrap()[0]
            .0
            .starts_with("GET /v1/models ")
    );
}

#[test]
fn the_target_is_the_picked_model_on_a_configured_endpoint() {
    let cfg = Config::default();
    let pick = |e: &str, m: &str| State {
        endpoint: e.into(),
        model: m.into(),
        ..State::default()
    };
    let picked = pick("oMLX", "gemma");
    let (ep, model) = target(&cfg, &picked).unwrap();
    assert_eq!((ep.name.as_str(), model), ("oMLX", "gemma"));
    let none = target(&cfg, &State::default()).unwrap_err().to_string();
    assert!(none.contains("pick"), "{none}");
    assert!(target(&cfg, &pick("gone", "gemma")).is_err());
}

#[tokio::test]
async fn server_errors_and_a_dead_endpoint_are_readable() {
    let err = json!({"error": {"message": "this model does not accept images"}}).to_string();
    let m = mock(vec![Reply::Plain(400, err)], "");
    let msg = ask(&m, 100).await.0.unwrap_err().to_string();
    assert_eq!(msg, "400 Bad Request: this model does not accept images");
    let dead = dead_endpoint("LM Studio");
    let r = Client::new()
        .unwrap()
        .stream(&dead, body(1), &mut |_| {}, &CancellationToken::default())
        .await;
    assert!(
        r.unwrap_err()
            .to_string()
            .starts_with("cannot reach LM Studio")
    );
}
