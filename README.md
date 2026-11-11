# Blitznir

<img src="assets/icon-1024.png" width="96" alt="Blitznir logo: a hammer whose shaft is a lightning bolt">

**Double-tap a key anywhere: dictate into any app, or rewrite, translate or chat about the
selected text in place. Local AI first: no account, no API key, no subscription, and nothing
leaves your machine unless you point it at a cloud endpoint. macOS and Linux (GNOME).**

https://github.com/user-attachments/assets/fcccda74-5fa9-45f8-a048-3cd017899496

## Why

Dictation that sends your voice to a server is slow to start and not yours. Rewriting text by
copying it into a chat window and back is five steps too many. Blitznir does both from a
double-tap, in whatever app you are typing in:

- Speech is recognized on your machine with Parakeet Ultra (25 European languages).
- Rewrite, translate and chat go to an OpenAI-compatible server you choose: LM Studio, oMLX,
  a llama.cpp server, Ollama. With a local model nothing leaves the machine.
- The result replaces the selection: no copy, no paste, no window switching.

One native Rust app with a menu-bar icon; its windows are drawn by [Freya](https://github.com/marc2332/freya)
(Skia). No webview, no JavaScript, no Electron.

## Features

- **Live dictation** types while you speak; **batch dictation** types it all when you stop.
- **Rewrite every dictation** (optional prompt): the rewrite replaces what you said, and a box by
  the text offers your words back (**Revert**); in **Suggest** mode your words stay and it offers
  the rewrite; not automatic, it offers **Rewrite** at once. Two ⌘Z undo either.
- **Rewrite in place**: select text, open the panel, press Return. An empty field cleans up the
  text; an instruction ("shorter", "bullet points") does that. The result replaces the selection.
- **Translate** between your two languages, in place; **quick translate** on its own hotkey.
  German and English by default; Settings changes the pair.
- **Buttons** with your own prompts (Summarize, Formal, …), each with an optional hotkey.
- **Chat** about the selection, with images (paste one, or screenshot a region); **Write out**
  pastes an answer back. Chats are saved and searchable. **Think** (the bulb by the model) lets
  a reasoning model think before it answers; off by default. Its thinking streams in, folded
  above the answer. One that loops or thinks past the token ceiling is answered without it.
  A cancelled or failed question stays in its chat, marked, with a Send again; Retry asks for a
  new last answer. Images open large as attached (Fit, 1:1, wheel zoom, drag) and copy.
- **Vocabulary**: spoken forms become written ones ("o m l x" → oMLX), and spellings are kept.
- **History** of dictations and replacements; **Undo last replace**.
- **Effects** while it works: a beam of light (or lightning) onto the text, a soft wave under
  the line you dictate into, and one where a chat answer will appear. Sound is off by default.
- **Settings** as a searchable form that saves itself, or as the raw `config.toml`, which
  documents itself. Hotkeys are recorded by pressing them. **Check for updates** in About asks
  GitHub only when you press it.

## Hotkeys

| Action | macOS | Linux |
|---|---|---|
| Live dictation (again stops) | double-tap Right Option or Right Ctrl | double-tap Right Ctrl |
| Batch dictation | double-tap Right Command | double-tap Right Alt |
| Panel on the selected text | double-tap Right Shift | double-tap Right Shift |
| Chat, quick action, quick translate, undo, button hotkeys | off by default | off by default |

Any modifier (double-tap) or F1–F24 (single press) can be bound in Settings. Esc cancels a
running answer or effect, else hides the panel.

## Install on macOS

macOS 13 or later (built and used on Apple Silicon), the Xcode command line tools and Rust 1.90
or later ([rustup](https://rustup.rs)).

```sh
scripts/install-macos.sh            # build, sign, install /Applications/Blitznir.app
scripts/install-macos.sh --force    # replace an existing one (the old one goes to the Trash)
```

Open the app: its menu lists the permissions it still needs (Input Monitoring for the hotkeys,
Accessibility for typing and paste-back, Screen Recording for screenshots only; the microphone is
asked on the first recording). The first start downloads the speech model (2.6 GB). Tick
**Start at login** in the menu.

**Signing.** Without a certificate the app is signed ad hoc, and macOS asks for every permission
again after each rebuild. To keep them, create a certificate once (Keychain Access → Certificate
Assistant → Create a Certificate: Identity Type "Self-Signed Root", Certificate Type "Code
Signing") and build with `BLITZNIR_SIGN="its name" scripts/install-macos.sh`.

## Install on Linux

```sh
scripts/install-linux.sh     # packages (apt or dnf), input group, /dev/uinput rule, build, install
```

It asks for sudo. **Log out and back in** afterwards: the `input` group lets the hotkeys read
`/dev/input` (and every process of yours read every keystroke). On vanilla GNOME enable the
AppIndicator extension, or the tray icon shows nowhere. Typing uses the desktop's keyboard layout
(its first, if it has several); `[linux] xkb_layout` names another. Then run `blitznir --check`.

Linux limits: F13–F24 also reach the desktop (keys are never swallowed), so prefer double-taps;
Translate always pastes back; the effects land mid-screen and there is no dictation wave (Wayland
says nothing about the caret); the Revert box is macOS only. Apps that share no selection (COSMIC's
own) are asked to copy with `[linux] copy_keys` (Ctrl+Insert) unless one was just made; a terminal
with nothing selected then shows "5~" (`copy_keys = ""` turns it off). Used on Ubuntu 26.04, GNOME 50
(arm64) and Pop!_OS 24.04, COSMIC (x86_64); CI builds and tests on x86_64 Ubuntu; KDE and X11 are
untested.

## Configuration and data

Everything lives in one folder: `~/Library/Application Support/Blitznir` on macOS,
`~/.local/share/Blitznir` on Linux (`BLITZNIR_DATA_DIR` overrides it), personal files written 0600.

| File | What |
|---|---|
| `config.toml` | all settings, with comments (Settings edits this file) |
| `state.json` | the picked model, the panel's zoom, the sidebar's width and folded lists |
| `history.json` | recent dictations and replacements |
| `recording.txt` | the text of a dictation in progress, recovered at the next start after a crash |
| `chats.json`, `images/` | saved chats and their images |
| `models/parakeet-ultra/` | the speech model, `fp32/` (2.6 GB) or `int8/` (670 MB) |
| `blitznir.log` | the log |

## Privacy

- Speech never leaves the machine. The model is downloaded once from Hugging Face.
- Text goes only to the endpoints in `config.toml`; the defaults are on `127.0.0.1`.
- No telemetry. Check for updates contacts GitHub only when you press it.

## Troubleshooting

- `blitznir --check` prints the data folder, whether `config.toml` parses, the model, each
  permission and a short microphone test.
- Hotkeys never fire: grant Input Monitoring (macOS), or join the `input` group and log in again.
- Nothing is typed: grant Accessibility (macOS); on Linux `ls -l /dev/uinput` must show `input`.
- Anything else: `blitznir.log` in the data folder; `RUST_LOG=debug` logs more.

## Development

- `scripts/check.sh` is the gate (fmt, clippy `-D warnings`, tests, size limits); CI runs it on
  macOS and Ubuntu. `scripts/linux-check.sh` checks the Linux side in Docker from a Mac.
- `crates/app/tests/ui.rs` drives the real UI headless: no window, nothing on your screen.
- `cargo run -p blitznir` runs it unbundled (the terminal's permissions);
  `blitznir transcribe file.wav` turns a 16 kHz WAV into text; `blitznir --help` lists the rest.
  Like the app, these commands write a default `config.toml` and download the speech model
  (2.6 GB) the first time.
- [CLAUDE.md](CLAUDE.md): the rules for changing this repo (for people and coding agents).

## Credits

- Speech: [Parakeet Ultra](https://huggingface.co/moondream/parakeet-ultra) by moondream, a post-train of
  [NVIDIA Parakeet TDT 0.6B v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3), [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/);
  ONNX by [Masterx](https://huggingface.co/Masterx/parakeet-tdt-0.6b-ultra-onnx) on istupakov's graphs, run with parakeet-rs.
- Windows: [Freya](https://github.com/marc2332/freya) by [@marc2332](https://github.com/marc2332). Sounds: thunder-synth.

## License

[Apache-2.0](LICENSE). The speech model keeps its own license (CC BY 4.0, above).
