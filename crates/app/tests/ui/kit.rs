//! What the UI tests share: a panel over the Fake platform in a temporary
//! data dir, made-up chats, local endpoints, and frames to look at.

use blitznir_app::panel::{Hooks, Panel, Parts};
use blitznir_app::tray::Tray;
use blitznir_core::config::{Config, Endpoint, Paths};
use blitznir_core::lock;
use blitznir_core::platform::Platform;
use blitznir_core::platform::fake::FakePlatform;
use blitznir_core::store::{Chat, Message, State, Store};
use freya::prelude::{Area, LaunchProxy};
use freya_testing::prelude::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

/// The panel's own size, in points.
pub const WINDOW: (f32, f32) = (760.0, 540.0);
pub const LAST_LINE: &str = "That is the whole answer.";

/// A panel over a fresh data dir holding `chats`.
pub fn panel(proxy: &LaunchProxy, name: &str, chats: &[Chat]) -> Arc<Panel> {
    panel_over(proxy, name, chats, fake(), Config::default())
}

/// A fresh Fake platform, for [`panel_over`].
pub fn fake() -> Arc<FakePlatform> {
    Arc::new(FakePlatform::new())
}

/// [`panel`] over `platform` and `cfg`.
pub fn panel_over(
    proxy: &LaunchProxy,
    name: &str,
    chats: &[Chat],
    platform: Arc<FakePlatform>,
    cfg: Config,
) -> Arc<Panel> {
    panel_seeded(proxy, name, chats, platform, cfg, |_| {})
}

/// The data folder of the panel made as `name`.
pub fn data_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("blitznir-ui-{name}-{}", std::process::id()))
}

/// [`panel_over`], its store filled by `seed` first (history rows).
pub fn panel_seeded(
    proxy: &LaunchProxy,
    name: &str,
    chats: &[Chat],
    platform: Arc<FakePlatform>,
    cfg: Config,
    seed: impl FnOnce(&Store),
) -> Arc<Panel> {
    let dir = data_dir(name);
    let _ = std::fs::remove_dir_all(&dir);
    let paths = Paths::new(&dir);
    let store = Arc::new(Store::new(paths.clone(), 10).expect("a store in the temp dir"));
    for chat in chats {
        store.save_chat(&mut chat.clone()).expect("chat saved");
    }
    seed(&store);
    if let Some((ep, model)) = (cfg.endpoints.iter()).find_map(|e| Some((e, e.models.first()?))) {
        let pick = |s: &mut State| (s.endpoint, s.model) = (ep.name.clone(), model.clone());
        store.update_state(pick).expect("the model picked");
    }
    let platform: Arc<dyn Platform> = platform;
    let tray = Tray::new(platform.clone(), proxy.clone());
    let hooks = Hooks {
        busy: Box::new(|_| {}),
        config: Box::new(|_| {}),
    };
    let parts = Parts {
        platform,
        store,
        paths,
        cfg,
        tray,
        proxy: proxy.clone(),
    };
    Panel::new(parts, hooks).expect("the panel")
}

pub fn message(role: &str, content: String) -> Message {
    Message {
        role: role.into(),
        content,
        ts: "2026-10-02T12:00:00".into(),
        images: vec![],
        thinking: String::new(),
        unthought: blitznir_core::store::Unthought::No,
        unanswered: None,
    }
}

/// A question and an answer several screens long, in the shape models
/// write: headings, bold labels, bullets, inline code, a rule.
pub fn long_chat() -> Chat {
    let mut answer = String::from("Both work; which one fits depends on the caller.\n\n");
    for (i, name) in ["Bulk setter", "Fluent setters", "Builder struct"]
        .iter()
        .enumerate()
    {
        answer += &format!(
            "### ({}) `{}` ({name})\n\n**Pros:**\n\n",
            i + 1,
            name.to_lowercase()
        );
        answer += "* **Concise:** one call when every `TransactionMetadata` field is known.\n";
        answer += "* **Atomic:** the settings are applied as a single unit.\n\n**Cons:**\n\n";
        answer += "* **Less flexible:** setting only the `gas_budget` means building it all.\n\n";
        answer +=
            "**Use this when:** the details are known up front and the code should be short.\n\n";
    }
    answer +=
        &format!("---\n\n## Conclusion\n\nPick the fluent setters by default.\n\n{LAST_LINE}");
    let mut chat = Chat::new();
    chat.name = "Which setter pattern?".into();
    chat.messages = vec![
        message(
            "user",
            "what's the better pattern for TransactionMetadata?".into(),
        ),
        message("assistant", answer),
    ];
    chat
}

/// Layout, the async tasks (the jump to the end) and a few frames.
pub fn settle(t: &mut TestingRunner) {
    t.poll(Duration::from_millis(4), Duration::from_millis(60));
}

/// Frames at 30 a second for `ms`, as the window draws them: the tip's
/// spring steps once a frame.
pub fn animate(t: &mut TestingRunner, ms: u64) {
    for _ in 0..ms / 33 {
        t.handle_events_immediately();
        t.sync_and_update();
        t.render();
        std::thread::sleep(Duration::from_millis(33));
    }
}

/// `UI_SHOTS=<dir>`: the frame as `<dir>/<name>.png`, to look at.
pub fn shot(t: &mut TestingRunner, name: &str) {
    if let Ok(dir) = std::env::var("UI_SHOTS") {
        t.render_to_file(std::path::Path::new(&dir).join(format!("{name}.png")));
    }
}

/// The form's paragraph holding `text`: its highlights and where it is.
pub fn paragraph_with(t: &TestingRunner, text: &str) -> Option<(Vec<(usize, usize)>, Area)> {
    t.find(|node, element| {
        Paragraph::try_downcast(element)
            .filter(|p| p.spans.iter().any(|s| s.text.contains(text)))
            .map(|p| (p.highlights, node.layout().area))
    })
}

/// Press the label starting with `start`.
pub fn press_label_starting(t: &mut TestingRunner, start: &str) {
    let at = label_where(t, |l| l.starts_with(start));
    press_at(
        t,
        at.unwrap_or_else(|| panic!("no label starting {start:?}")),
    );
}

/// Where the first label whose text `matches` is.
pub fn label_where(t: &TestingRunner, matches: impl Fn(&str) -> bool) -> Option<Area> {
    t.find(|node, e| {
        Label::try_downcast(e)
            .filter(|l| matches(&l.text))
            .map(|_| node.layout().area)
    })
}

/// Where the label reading exactly `text` is.
pub fn label_at(t: &TestingRunner, text: &str) -> Option<Area> {
    label_where(t, |l| l == text)
}

/// A click in the middle of `at`.
fn press_at(t: &mut TestingRunner, at: Area) {
    let centre = (f64::from(at.center().x), f64::from(at.center().y));
    t.move_cursor(centre);
    t.click_cursor(centre);
    settle(t);
}

/// Press the label reading exactly `text` (a button's or a sidebar entry's).
pub fn press_label(t: &mut TestingRunner, text: &str) {
    let at = label_at(t, text);
    press_at(t, at.unwrap_or_else(|| panic!("no label {text:?}")));
}

/// A config whose one endpoint is `url` with one model, and no effect.
pub fn local_config(url: &str) -> Config {
    let mut cfg = Config {
        endpoints: vec![Endpoint {
            name: "mock".into(),
            url: url.into(),
            key: String::new(),
            models: vec!["m".into()],
            vision: None,
        }],
        ..Config::default()
    };
    cfg.panel.effect = blitznir_core::config::Effect::Off;
    cfg.dictation.revert = false;
    cfg.dictation.rewrite = "Fix:\n{text}".into();
    cfg
}

/// An endpoint that takes every connection and never answers: a job on it
/// runs until it is cancelled.
pub fn silent_endpoint() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
    let url = format!("http://{}/v1", listener.local_addr().expect("its address"));
    std::thread::spawn(move || {
        let held: Vec<_> = listener.incoming().collect();
        drop(held);
    });
    url
}

pub fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let t0 = Instant::now();
    while !done() {
        // generous: CI runs the UI tests beside every other test
        assert!(t0.elapsed() < Duration::from_secs(15), "waited for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A local server that hands every connection to `answer` on a thread of
/// its own; its URL, as an endpoint's.
fn serve(answer: impl Fn(TcpStream) + Send + Sync + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
    let url = format!("http://{}/v1", listener.local_addr().expect("its address"));
    let answer = Arc::new(answer);
    std::thread::spawn(move || {
        for s in listener.incoming().flatten() {
            let answer = answer.clone();
            std::thread::spawn(move || answer(s));
        }
    });
    url
}

/// Read a request to its end (headers, then Content-Length of body).
fn read_request(s: &TcpStream) {
    let mut r = BufReader::new(s.try_clone().expect("the stream"));
    let mut len = 0;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        if let Some(n) = line.to_lowercase().strip_prefix("content-length:") {
            len = n.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; len];
    let _ = r.read_exact(&mut body);
}

const STREAM_HEAD: &str =
    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";

/// An endpoint that answers every request with `text`, streamed.
pub fn answering_endpoint(text: &'static str) -> String {
    serve(move |mut s| {
        read_request(&s);
        // `text` has no quote or backslash to escape
        let delta = format!(r#"{{"choices": [{{"delta": {{"content": "{text}"}}}}]}}"#);
        let _ = write!(s, "{STREAM_HEAD}data: {delta}\n\ndata: [DONE]\n\n");
    })
}

/// An endpoint that waits `first`, then streams `text` a word every `every`,
/// holding after the first `held` words until the sender is dropped:
/// a slow machine cannot stream more words into what a test looks at.
pub fn slow_endpoint(
    text: &'static str,
    first: Duration,
    every: Duration,
    held: usize,
) -> (String, mpsc::Sender<()>) {
    let (release, wait) = mpsc::channel::<()>();
    let wait = Mutex::new(wait);
    let url = serve(move |mut s| {
        read_request(&s);
        let _ = write!(s, "{STREAM_HEAD}");
        let _ = s.flush();
        std::thread::sleep(first);
        for (i, w) in text.split(' ').enumerate() {
            if i == held {
                let _ = lock(&wait).recv();
            }
            // `text` has no quote or backslash to escape
            let delta = format!(r#"{{"choices": [{{"delta": {{"content": "{w} "}}}}]}}"#);
            let _ = write!(s, "data: {delta}\n\n");
            let _ = s.flush();
            std::thread::sleep(every);
        }
        let _ = write!(s, "data: [DONE]\n\n");
    });
    (url, release)
}

/// An endpoint that streams `trace` as reasoning, a word every `every`,
/// then answers `text`.
pub fn thinking_endpoint(trace: &'static str, text: &'static str, every: Duration) -> String {
    serve(move |mut s| {
        read_request(&s);
        let _ = write!(s, "{STREAM_HEAD}");
        for w in trace.split(' ') {
            // `trace` has no quote or backslash to escape
            let delta = format!(r#"{{"choices": [{{"delta": {{"reasoning_content": "{w} "}}}}]}}"#);
            let _ = write!(s, "data: {delta}\n\n");
            let _ = s.flush();
            std::thread::sleep(every);
        }
        let delta = format!(r#"{{"choices": [{{"delta": {{"content": "{text}"}}}}]}}"#);
        let _ = write!(s, "data: {delta}\n\ndata: [DONE]\n\n");
    })
}

/// An endpoint that answers every request with a server error, `after` it.
pub fn failing_endpoint(after: Duration) -> String {
    serve(move |mut s| {
        std::thread::sleep(after);
        let body = r#"{"error": {"message": "the model crashed"}}"#;
        let _ = write!(
            s,
            "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    })
}
