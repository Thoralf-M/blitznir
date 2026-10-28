//! In-memory [`Platform`] and [`Recognizer`] for tests.

use super::{Bounds, CopyToken, Focus, KeyEvent, Platform, Recognizer, Rect, Token};
use crate::{Result, lock};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

type Hook = Box<dyn FnMut(&str) -> Result<()> + Send>;

/// A text field in memory: `type_text` appends, `backspace` deletes.
#[derive(Default)]
pub struct FakePlatform {
    screen: Mutex<String>,
    clipboard: Mutex<String>,
    /// What `finish_copy` returns.
    pub selection: Mutex<Option<String>>,
    /// What `focused_editable` returns.
    pub editable: Option<bool>,
    /// What `selection_bounds` returns.
    pub bounds: Option<Bounds>,
    /// What `caret` returns.
    pub caret: Option<Rect>,
    /// What `mouse` returns.
    pub mouse: Option<(f64, f64)>,
    /// What `clipboard_image` returns.
    pub image: Mutex<Option<Vec<u8>>>,
    /// Sent by `listen`, in order.
    pub keys: Mutex<Vec<KeyEvent>>,
    /// Every app `paste_into` was asked to paste into, in order.
    pub pasted_into: Mutex<Vec<Focus>>,
    /// Every app `undo` was sent to, in order.
    pub undone_in: Mutex<Vec<Focus>>,
    /// Our own window is in front (the effect's overlay took the key
    /// focus): `focused_app` names no app, as macOS's leaves ours out.
    pub ours_in_front: AtomicBool,
    /// What `prefers_dark` returns.
    pub dark: Option<bool>,
    /// `[linux] xkb_layout` of the last `configure`.
    pub layout: Mutex<Option<String>>,
    /// The screen before the last paste: what `undo` brings back.
    before_paste: Mutex<Option<String>>,
    on_type: Mutex<Option<Hook>>,
}

impl FakePlatform {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn screen(&self) -> String {
        lock(&self.screen).clone()
    }

    pub fn clipboard(&self) -> String {
        lock(&self.clipboard).clone()
    }

    /// Called with the text of every `type_text`, before it lands on the
    /// screen; an error makes that `type_text` fail and type nothing.
    pub fn on_type(&self, hook: impl FnMut(&str) -> Result<()> + Send + 'static) {
        *lock(&self.on_type) = Some(Box::new(hook));
    }
}

impl Platform for FakePlatform {
    fn listen(&self, tx: Sender<KeyEvent>) -> Result<()> {
        for ev in lock(&self.keys).drain(..) {
            let _ = tx.send(ev);
        }
        Ok(())
    }

    fn type_text(&self, s: &str) -> Result<()> {
        if let Some(hook) = lock(&self.on_type).as_mut() {
            hook(s)?;
        }
        lock(&self.screen).push_str(s);
        Ok(())
    }

    fn backspace(&self, n: usize) -> Result<()> {
        let mut screen = lock(&self.screen);
        for _ in 0..n {
            screen.pop();
        }
        Ok(())
    }

    fn wait_modifiers_released(&self) {}

    fn focused_app(&self) -> Focus {
        Focus((!self.ours_in_front.load(Ordering::SeqCst)).then_some(1))
    }

    fn focused_editable(&self, _f: &Focus) -> Option<bool> {
        self.editable
    }

    fn begin_copy(&self) -> CopyToken {
        CopyToken(0)
    }

    fn finish_copy(&self, _t: CopyToken) -> Option<String> {
        lock(&self.selection).clone()
    }

    fn selection_bounds(&self, _f: &Focus) -> Option<Bounds> {
        self.bounds
    }

    fn caret(&self, _f: &Focus) -> Option<Rect> {
        self.caret
    }

    fn mouse(&self) -> Option<(f64, f64)> {
        self.mouse
    }

    fn set_clipboard(&self, s: &str) -> Result<()> {
        *lock(&self.clipboard) = s.to_string();
        Ok(())
    }

    fn clipboard_image(&self) -> Option<Vec<u8>> {
        lock(&self.image).clone()
    }

    fn show_quietly(&self) {}

    fn configure(&self, cfg: &crate::config::Config) {
        *lock(&self.layout) = Some(cfg.linux.xkb_layout.clone());
    }

    fn prefers_dark(&self) -> Option<bool> {
        self.dark
    }

    fn backspace_into(&self, _f: &Focus, n: usize) -> Result<bool> {
        self.backspace(n)?;
        Ok(true)
    }

    fn paste_into(&self, f: &Focus) -> Result<bool> {
        lock(&self.pasted_into).push(*f);
        *lock(&self.before_paste) = Some(self.screen());
        let text = self.clipboard();
        self.type_text(&text)?;
        Ok(true)
    }

    fn undo(&self, f: &Focus) -> Result<bool> {
        lock(&self.undone_in).push(*f);
        if let Some(before) = lock(&self.before_paste).take() {
            *lock(&self.screen) = before;
        }
        Ok(true)
    }

    fn capture(&self, _cmd: &str, out: &Path) -> Result<bool> {
        std::fs::write(out, b"")?;
        Ok(true)
    }

    fn open_url(&self, _url: &str) -> Result<()> {
        Ok(())
    }

    fn play_sound(&self, _path: &Path) -> Result<Option<std::process::Child>> {
        Ok(None)
    }

    fn missing_permissions(&self) -> Vec<&'static str> {
        vec![]
    }
}

type Decode = Box<dyn FnMut(&[f32]) -> Result<Vec<Token>> + Send>;

/// A recognizer that answers with whatever its closure says.
pub struct FakeRecognizer {
    decode: Decode,
    /// How often `transcribe` ran.
    pub calls: usize,
}

impl FakeRecognizer {
    pub fn new(decode: impl FnMut(&[f32]) -> Result<Vec<Token>> + Send + 'static) -> Self {
        Self {
            decode: Box::new(decode),
            calls: 0,
        }
    }
}

impl Recognizer for FakeRecognizer {
    fn transcribe(&mut self, pcm16k: &[f32]) -> Result<Vec<Token>> {
        self.calls += 1;
        (self.decode)(pcm16k)
    }
}

const SR: f64 = 16_000.0;

/// Synthetic speech: words with absolute (start, end) seconds. Every sample
/// of [`Timeline::audio`] encodes its own index — voiced ≈ 0.2 (loud), silent
/// < 0.001 — so [`Timeline::tokens`] can tell which stretch of the recording a
/// buffer is and answer like a perfect recognizer would.
#[derive(Clone)]
pub struct Timeline {
    words: Arc<Vec<(String, f64, f64)>>,
}

impl Timeline {
    pub fn new(words: &[(&str, f64, f64)]) -> Self {
        Self {
            words: Arc::new(
                words
                    .iter()
                    .map(|(w, s, e)| ((*w).to_string(), *s, *e))
                    .collect(),
            ),
        }
    }

    /// `total_s` seconds, loud inside each word and silent between them.
    pub fn audio(&self, total_s: f64) -> Vec<f32> {
        (0..(total_s * SR).round() as usize)
            .map(|i| {
                let t = i as f64 / SR;
                let voiced = self.words.iter().any(|(_, s, e)| *s <= t && t < *e);
                (if voiced {
                    0.2 + i as f64 * 3e-7
                } else {
                    i as f64 * 1e-9
                }) as f32
            })
            .collect()
    }

    /// Start of `pcm` in the recording, in seconds.
    pub fn offset(pcm: &[f32]) -> f64 {
        let v = f64::from(pcm.first().copied().unwrap_or(0.0));
        let i = if v >= 0.1 { (v - 0.2) / 3e-7 } else { v / 1e-9 };
        i.round() / SR
    }

    /// Finished words in `pcm` with their end times relative to it; a word
    /// still being spoken at the end comes out cut short, as a guess.
    pub fn tokens(&self, pcm: &[f32]) -> Vec<Token> {
        let start = Self::offset(pcm);
        let end = start + pcm.len() as f64 / SR;
        let mut out = vec![];
        for (w, s, e) in self.words.iter() {
            if *e <= start + 1e-6 || *s >= end {
                continue;
            }
            if *e <= end + 1e-6 {
                out.push(Token::new(&format!(" {w}"), (*e - start) as f32));
            } else {
                let n = (w.chars().count() as f64 * ((end - s) / (e - s)).clamp(0.0, 1.0)) as usize;
                let guess: String = w.chars().take(n.max(1)).collect();
                out.push(Token::new(&format!(" {guess}"), (end - start) as f32));
            }
        }
        out
    }

    pub fn recognizer(&self) -> FakeRecognizer {
        let t = self.clone();
        FakeRecognizer::new(move |pcm| Ok(t.tokens(pcm)))
    }
}
