# Blitznir — rules for changing this repo

Local dictation + rewrite/translate/chat for macOS and Linux. Rust workspace, one app on Freya
(Skia, no webview). The why of a decision is in the code's comments.

## Map (read before editing)

- crates/core: ALL logic. No freya/cpal/ort/objc/evdev deps, no unsafe. Tested headless.
- crates/shared (package blitznir-shared, lib `blitznir`): the non-UI half — audio.rs, stt.rs,
  session.rs, logging.rs, cli.rs, speaker.rs (the beam's sound out, one gapless cpal stream fed
  sample by sample), platform/{macos,linux}.rs (the ONLY place for OS code and unsafe, besides
  the app's wayland.rs). No UI toolkit (check.sh fails if freya enters it).
- crates/app (package and binary blitznir, Freya): glue only — daemon.rs, tray.rs, autostart.rs,
  panel/ (the flows over core; `view::Ui` is what the window shows), ui/ (the components: dumb
  renderers of `Ui`, no prompts, HTTP, persistence or state beyond the open view), zap.rs + zap/
  (the effects), wayland.rs (Linux). A library (lib.rs) with a thin main.rs, so tests/ui.rs can
  build the panel.
- Seams: core::platform::{Platform, Recognizer}. Every method has macos, linux AND Fake impls.
  OS processes too: `Platform::capture`/`open_url`; shared's platform/mod.rs holds what both
  OSes share.
- Prompts live in crates/core/src/tasks.rs only. Config keys: crates/core/src/config.rs and its
  default file, DEFAULT_TOML, assembled from crates/core/src/config/*.toml.
- Lock a Mutex with `blitznir_core::lock` (poison-tolerant), never a local helper or `.lock().unwrap()`.
- thunder-synth (crates.io, its own repo): the lightning's thunder and rain. Change it there.

## Testing the UI

tests/ui.rs runs the real UI headless (freya-testing) over the Fake platform and made-up chats:
no window, nothing on anyone's screen. Reproduce a UI bug there first, with a test that fails
without the fix; `UI_SHOTS=<dir>` saves its frames as PNGs to look at. Never drive a real window
with synthetic mouse or keyboard events: someone is using that machine.

## Limits (scripts/check.sh fails the build)

- .rs file ≤ 400 lines; function ≤ 80 lines (clippy too_many_lines); README ≤ 150 lines.
- No unwrap() outside tests (expect("why") allowed).
- No println!/dbg!; use log::.
- Format with nightly: `cargo +nightly fmt --all -- --config imports_granularity=Crate`.

## Never add

- Settings that live anywhere but config.toml. The Settings form (ui/form.rs) only writes
  config.toml, which stays the one truth and always open as whole text in the Settings tab.
- Emoji anywhere in the UI: tray titles, status lines, buttons, menus, log lines shown to the
  user. Plain words ("Listening…", "Rewrite failed: …"). The hand-drawn SVG icons in
  ui/icons.rs are wanted, with a tooltip or a short label where the meaning is not obvious.
- Animations or overlays other than the effects below.
- A second UI framework, npm, a bundler, Python (assets/make_logo.py only renders the logo).
- Backend-specific code (LM Studio/oMLX/llama.cpp special cases); OpenAI-compatible only.
- Starting/stopping/loading/unloading other programs or models.
- Async outside core/llm.rs (and its modules), bar Freya's own tasks on the UI thread (frame
  timers, wake-ups). Global mutable state. A trait with one impl (except the seams).
- A crate for < 50 lines of code, or without a one-line justification in the commit message.
- Comments narrating history ("used to…"). Put the why of a measured constant next to it.
- Backward-compat shims or config migrations.

## The effects (the only animation)

None touches text or focus, takes a key or a click (bar the revert box's own), or blurs per
frame (glows are images made once); ≤ 30 fps while something moves, 10 idle, none when gone.

- The working effect, beam or lightning (`[panel] effect`, or `"off"`): zap.rs + zap/,
  geometry in core's sky.rs, the beam's timeline in core's beam.rs, sounds from thunder-synth and
  core's beam_sound.rs (`[panel] effect_volume`, the volume, default 0). Its overlay holds key focus only
  in the hidden panel's place, for Esc, and never at the paste.
- The revert box (`[dictation] revert`, panel/revert.rs + ui/revert.rs): the one window over the
  user's app; shown without key focus, gone at the first key or click outside it.
- The dictation wave (`[panel] dictation_effect`, zap/caret*, macOS): under the caret line while a
  recording runs, in a line-sized, click-through, never-active window; in our own composer
  ui/dictation.rs draws it. The thinking wave (`[panel] thinking_effect`, ui/thinking.rs): in the
  chat answer's slot until the first word. Both draw through zap/wave/ and core's wave.rs.
- The panel's zoom (a double click on its header, ui/zoom.rs + core's zoom.rs, macOS): the window
  moves its own frame to the screen's free area and back over 0.22 s at 60 steps a second, each
  laid out and drawn; AppKit's zoom stretched a picture of it. Linux maximizes.

## Adding a feature

1. Can an existing path do it (config value, prompt, existing command)? Then do that.
2. Logic goes in core with tests first (Fake platform/recognizer).
3. New OS capability: add a Platform method + macos + linux + Fake in ONE change,
   or document in README why Linux lacks it.
4. UI: a method on the panel (panel/), a few lines in the component (ui/), a test in tests/ui.rs.
5. Config key: struct field + serde default + commented line in DEFAULT_TOML + a form entry.
6. User-visible: ≤ 3 README lines.

## Done means

scripts/check.sh passes (fmt, clippy -D warnings, core and UI tests, app build, size gates),
and for anything touching platform/, audio, stt or ui: the smoke list below, run by hand on the
OS you changed, is noted in the commit.

## Smoke list

1 live dictation into a text editor, with pauses · 2 batch · 3 kill -9 mid-recording → recovered ·
4 rewrite ⏎ empty · 5 instruction · 6 translate in an editable field → replaced; on a web page
(mac) → shown + Write out · 7 chat + image paste · 8 capture with the panel hidden ·
9 cancel mid-stream · 10 endpoint down → error shown · 11 Settings: a form change saved, broken
TOML refused in the text · 12 restart keeps permissions (mac) · 13 Linux: items 1–7 on a VM

## Freya

- An exact crates.io version in crates/app/Cargo.toml (freya and freya-testing alike): every rc
  moves APIs. Its skia bindings need `prettyplease` 0.3 in Cargo.lock (0.2 does not build against
  the `syn` 3 that serde pulls in).
- Never write Freya from memory: models know the dead 0.3 API. Clone the tag in use
  (`git clone --depth 1 -b v0.5.2 https://github.com/marc2332/freya`) and read its
  website/public/llms.txt, examples/ and crates/freya/src/_docs. Custom drawing is a
  rect's `.background(RenderCallback::new(…))` (rc.9 removed `canvas`).
- wayland.rs (Linux, the selection on winit's Wayland connection) is crates/app's one OS file and
  its one place allowed unsafe.
- macOS bundle: scripts/bundle-macos.sh → target/release/bundle/Blitznir.app. It owns the bundle
  id `dev.blitznir.app` and minimum macOS, and signs with `BLITZNIR_SIGN` (a certificate's name;
  unset = ad hoc). The privacy grants hang on identifier + certificate: changing either asks for
  every grant again. Info.plist and entitlements.plist are in crates/app.
- Linux under Freya: GTK's Wayland connection never has focus, so selection and clipboard go
  through wayland.rs; Wayland neither hides, raises nor places a window, so windows are destroyed
  and made anew; GNOME backs a fullscreen window with opaque black.
