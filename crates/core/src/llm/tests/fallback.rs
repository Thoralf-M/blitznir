//! The think-off fallback: a chat that kept thinking past the ceiling.

use super::*;
use crate::llm::looping::tests::fresh;
use crate::store::Unthought;

#[tokio::test]
async fn thinking_past_the_ceiling_is_answered_without_thinking() {
    let looping = || {
        Reply::Sse(vec![
            thinking("reasoning", "Wait, I'll check…"),
            finish("length"),
            done(),
        ])
    };
    let plain = Reply::Sse(vec![content("ok"), done()]);
    let m = mock(vec![looping(), looping(), plain], "");
    let mut b = body(4096);
    b["chat_template_kwargs"]["enable_thinking"] = true.into();
    let r = Client::new()
        .unwrap()
        .stream(&m.ep, b, &mut |_| {}, &CancellationToken::default())
        .await;
    assert_eq!(r.unwrap(), "ok");
    m.server.join().unwrap();
    let tries: Vec<(Value, Value)> = m
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|(_, b)| {
            (
                b["max_tokens"].clone(),
                b["chat_template_kwargs"]["enable_thinking"].clone(),
            )
        })
        .collect();
    let want = [
        (4096, true),
        (MAX_TOKENS_CEILING, true),
        (MAX_TOKENS_CEILING, false),
    ]
    .map(|(n, t)| (json!(n), json!(t)));
    assert_eq!(tries, want);
}

#[tokio::test]
async fn the_think_off_answer_keeps_the_trace_that_ran_out() {
    let looping = |t: &str| Reply::Sse(vec![thinking("reasoning", t), finish("length"), done()]);
    let plain = Reply::Sse(vec![content("ok"), done()]);
    // a chat's budget is its cap: no larger retry, straight to no thinking
    let m = mock(vec![looping("Wait, I'll check…"), plain], "");
    let mut b = body(4096);
    b["chat_template_kwargs"]["enable_thinking"] = true.into();
    let a = chat(&m, b).await;
    let want = ("ok", "Wait, I'll check…", Unthought::RanOut);
    assert_eq!((a.text.as_str(), a.thinking.as_str(), a.unthought), want);
    m.server.join().unwrap();
}

#[tokio::test]
async fn the_think_off_retry_gets_the_trace_as_notes_cut_at_its_loop() {
    let head = fresh(600);
    let looped = format!("{head}{head}{head}");
    let looping = |t: &str| Reply::Sse(vec![thinking("reasoning", t), finish("length"), done()]);
    let plain = Reply::Sse(vec![content("ok"), done()]);
    let m = mock(vec![looping(&looped), plain], "");
    let mut b = body(4096);
    b["chat_template_kwargs"]["enable_thinking"] = true.into();
    b["messages"] =
        json!([{"role": "system", "content": "Be brief."}, {"role": "user", "content": "Why?"}]);
    assert_eq!(chat(&m, b).await.text, "ok");
    m.server.join().unwrap();
    let sent = m.requests.lock().unwrap();
    let systems: Vec<&str> = sent
        .iter()
        .map(|(_, b)| b["messages"][0]["content"].as_str().unwrap())
        .collect();
    assert_eq!(systems[0], "Be brief.", "no notes while it may think");
    let notes = format!("<notes>\n{head}\n</notes>");
    assert!(
        systems[1].starts_with("Be brief.\n\nYour earlier reasoning"),
        "{}",
        systems[1]
    );
    assert!(
        systems[1].ends_with(&notes),
        "cut where it repeats: {}",
        systems[1]
    );
    assert_eq!(
        sent[1].1["messages"].as_array().unwrap().len(),
        2,
        "one system message"
    );
}

#[tokio::test]
async fn a_looping_trace_is_dropped_and_answered_without_thinking_at_once() {
    const SAGE: &str = "Wait, I'll check if I should mention that I can write others.\n    Yes.\n\n    Okay.\n\n    Let's write.\n\n    ";
    let head = fresh(1500);
    let trace = format!("{head}{}", SAGE.repeat(60));
    let chars: Vec<char> = trace.chars().collect();
    let mut lines: Vec<String> = chars
        .chunks(8)
        .map(|c| thinking("reasoning_content", &c.iter().collect::<String>()))
        .collect();
    lines.extend([finish("length"), done()]);
    let plain = Reply::Sse(vec![content("ok"), done()]);
    let m = mock(vec![Reply::Sse(lines), plain], "");
    let mut b = body(4096);
    b["chat_template_kwargs"]["enable_thinking"] = true.into();
    b["messages"] = json!([{"role": "system", "content": "Be brief."}]);
    let (a, seen) = chat_seen(&m, b).await;
    m.server.join().unwrap();
    assert_eq!((a.text.as_str(), a.unthought), ("ok", Unthought::Looped));
    let kept = a.thinking.chars().count();
    assert!(
        kept < head.len() + 25 * SAGE.len(),
        "dropped early: {kept} chars"
    );
    assert_eq!(seen.last(), Some(&a.thinking), "streamed until dropped");
    let sent = m.requests.lock().unwrap();
    let tries: Vec<(Value, Value)> = (sent.iter())
        .map(|(_, b)| {
            (
                b["max_tokens"].clone(),
                b["chat_template_kwargs"]["enable_thinking"].clone(),
            )
        })
        .collect();
    assert_eq!(
        tries,
        [(json!(4096), json!(true)), (json!(4096), json!(false))],
        "no budget retry"
    );
    let notes = sent[1].1["messages"][0]["content"].as_str().unwrap();
    assert!(notes.contains(&format!("<notes>\n{head}")), "{notes}");
    assert!(
        notes.matches("Let's write.").count() <= 3,
        "cut where it loops: {notes}"
    );
}
