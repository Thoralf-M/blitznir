//! What the Settings form shows: every key of config.toml, in the file's
//! order, with the file's own comments cut down to one line.

use super::{Column, Field, Kind, Section};

const fn f(
    table: &'static str,
    key: &'static str,
    label: &'static str,
    help: &'static str,
    kind: Kind,
) -> Field {
    Field {
        table,
        key,
        label,
        help,
        kind,
        example: "",
        advanced: false,
        os: "",
    }
}

/// The two tables' titles, before SECTIONS: [[actions]], [[endpoints]].
pub const TABLES: [&str; 2] = ["Buttons", "Servers"];

pub const SECTIONS: &[Section] = &[
    Section {
        title: "Hotkeys",
        about: "Double-tap a modifier key, or press an F-key once. Record takes the next key you press.",
        fields: &[
            f("hotkeys", "live", "Live dictation", "Types while you speak; again stops", Kind::Keys),
            f("hotkeys", "batch", "Batch dictation", "Types it all when you stop", Kind::Keys),
            f("hotkeys", "rewrite", "Panel", "Rewrite, translate or chat about the selection", Kind::Keys),
            f("hotkeys", "chat", "Chat", "The panel, straight in a chat", Kind::Keys),
            f("hotkeys", "quick", "Quick action", "The first button on the selection, no panel", Kind::Keys),
            f("hotkeys", "translate", "Quick translate", "Translate the selection in place, no panel", Kind::Keys),
            f("hotkeys", "undo", "Undo replace", "Take back the last replace", Kind::Keys),
            f("hotkeys", "double_tap_ms", "Double-tap window (ms)", "Max time between the two taps, 100 to 2000", Kind::Int)
                .advanced(),
        ],
    },
    Section {
        title: "Dictation",
        about: "Speech becomes text on this computer; your voice is never sent anywhere.",
        fields: &[
            f("dictation", "model", "Speech model", "fp32: most accurate, 2.6 GB. int8: 670 MB, more mistakes", Kind::Choice(&["fp32", "int8"])),
            f("dictation", "auto_copy", "Copy every dictation", "Also put it on the clipboard", Kind::Bool),
            f("dictation", "rewrite", "Rewrite every dictation", "A prompt the picked model runs on each dictation when it ends; {text} is the dictation. Empty = off", Kind::Long)
                .example("Fix punctuation and obvious mistakes; change nothing else:\n{text}"),
            f("dictation", "rewrite_auto", "Rewrite automatically", "Off: the dictation is typed at once and a box by it offers Rewrite", Kind::Bool)
                .only_on("macos"),
            f("dictation", "revert", "Offer the dictation back", "After a rewrite, a box by the text shows what you said", Kind::Bool)
                .only_on("macos"),
            f("dictation", "rewrite_replaces", "Rewrite replaces the dictation", "Automatic rewrites only. Off: your words stay and the box offers the rewrite", Kind::Bool)
                .only_on("macos"),
            f("dictation", "rewrite_in_panel", "Rewrite in Blitznir's own field too", "⌘Z takes the rewrite back", Kind::Bool),
            f("dictation", "history_max", "History kept", "Recent results kept of each kind", Kind::Int).advanced(),
            f("dictation", "silence_rms", "Silence level", "A 50 ms block below this RMS counts as silence", Kind::Float).advanced(),
            f("dictation", "segment_max_s", "Segment cap (s)", "Lower to 10 if live text lags behind your voice; 4 to 60", Kind::Float).advanced(),
        ],
    },
    Section {
        title: "Hands-free",
        about: "Dictate without a key: a phrase starts it, another ends it, a third sends it. The microphone stays open while this is on.",
        fields: &[
            f("wake", "enabled", "Listen for the wake phrase", "Keeps the microphone open for a small listener (about 1 % of a core)", Kind::Bool),
            f("wake", "wake", "Wake phrase", "Starts a live dictation. Two English-sounding words are heard best", Kind::Text).example("hey blitz"),
            f("wake", "stop", "Stop phrase", "Ends it and is taken off the text; empty: only a pause ends it", Kind::Text).example("blitz stop"),
            f("wake", "send", "Send phrase", "Said on its own after a dictation: Return in that app; empty: off", Kind::Text).example("blitz send"),
            f("wake", "scratch", "Take-back phrase", "Said on its own, a pause around it: takes the last dictated sentence back; empty: off", Kind::Text).example("blitz scratch, blitz delete"),
            f("wake", "quiet_s", "Pause that ends it (s)", "Seconds without speech that end a dictation the wake phrase started; 1 to 30", Kind::Float).advanced(),
            f("wake", "send_within_s", "Send within (s)", "How long after a dictation the send phrase works; 0 to 3600", Kind::Float).advanced(),
        ],
    },
    Section {
        title: "Vocabulary",
        about: "Words the speech model gets wrong, and names the rewrite must not change.",
        fields: &[
            f("vocabulary", "replace", "Replace", "One per line: what it hears = what to type. Whole words, any case", Kind::Map)
                .example("o m l x = oMLX\nblitz near = Blitznir\nget hub = GitHub"),
            f("vocabulary", "terms", "Keep as written", "Names the rewrite keeps exactly, comma-separated", Kind::List)
                .example("PostgreSQL, Kubernetes, Freya"),
        ],
    },
    Section {
        title: "Translate",
        about: "Translate switches between these two: your language becomes the other, anything else becomes yours.",
        fields: &[
            f("translate", "home", "Your language", "Text in it becomes the other one", Kind::Text).example("German"),
            f("translate", "other", "Other language", "Anything else becomes your language", Kind::Text).example("English"),
            f("translate", "on_select", "Button on a mouse selection", "A Translate button above text you select with the mouse", Kind::Bool)
                .only_on("macos"),
        ],
    },
    Section {
        title: "Models",
        about: "Which of your servers' models the picker offers, and how chat answers.",
        fields: &[
            f("llm", "chat_reasoning", "Reason before chat answers", "More complete answers, slower on simple ones", Kind::Bool),
            f("llm", "chat_max_tokens", "Chat token budget", "Reasoning and answer together, 1024 to 262144", Kind::Int).advanced(),
            f("llm", "hide_models", "Hide models (regex)", "Model ids matching this are not offered", Kind::Text).advanced(),
            f("llm", "vision_models", "Vision models (regex)", "Ids matching this take images when the server does not say", Kind::Text).advanced(),
        ],
    },
    Section {
        title: "Chat",
        about: "How a chat about the selected text begins.",
        fields: &[f("prompts", "chat", "System message", "{text} is the selection; empty = built-in", Kind::Long)],
    },
    Section {
        title: "Effects",
        about: "Light, and sound if you want it, while Blitznir works on your text.",
        fields: &[
            f("panel", "effect", "Effect", "Over the text while a replace runs, from the menu-bar hammer", Kind::Choice(&["beam", "lightning", "off"])),
            f("panel", "effect_volume", "Sound", "A hum and a swell, or thunder", Kind::Percent),
            f("panel", "rain", "Rain", "With the lightning's thunder, at its volume", Kind::Bool),
            f("panel", "dictation_effect", "Wave while dictating", "Under the line you dictate into", Kind::Bool).only_on("macos"),
            f("panel", "thinking_effect", "Wave while a chat answers", "In the answer's place until its first word", Kind::Bool),
        ],
    },
    Section {
        title: "Other",
        about: "Everything that fits nowhere else.",
        fields: &[
            f("capture", "cmd", "Screenshot command", "{path} is the file to write", Kind::Text),
            f("linux", "xkb_layout", "Keyboard layout", "Empty: the desktop's (Wayland). Or an xkb name: de, us, …", Kind::Text)
                .example("de")
                .only_on("linux"),
            f("linux", "paste_keys", "Paste keys", "Terminals want ctrl+shift+v", Kind::Text).only_on("linux"),
            f("linux", "undo_keys", "Undo keys", "Sent by Undo last replace", Kind::Text).only_on("linux"),
            f("linux", "copy_keys", "Copy keys", "Copies the selection where the app shares none; empty: never", Kind::Text).only_on("linux"),
        ],
    },
];

/// The composer's buttons, in order: label, prompt, and which one Return
/// runs outside and inside a chat.
pub const ACTIONS: &[Column] = &[
    Column {
        key: "label",
        label: "Label",
        kind: Kind::Text,
    },
    Column {
        key: "prompt",
        label: "Prompt",
        kind: Kind::Long,
    },
    Column {
        key: "enter",
        label: "Return",
        kind: Kind::Pick,
    },
    Column {
        key: "chat_enter",
        label: "Return in chat",
        kind: Kind::Pick,
    },
    Column {
        key: "hotkey",
        label: "Hotkey: runs it on the selection, no panel",
        kind: Kind::Keys,
    },
];

/// The OpenAI-compatible servers.
pub const ENDPOINTS: &[Column] = &[
    Column {
        key: "name",
        label: "Name",
        kind: Kind::Text,
    },
    Column {
        key: "url",
        label: "URL, ending in /v1",
        kind: Kind::Text,
    },
    Column {
        key: "key",
        label: "API key (empty = none)",
        kind: Kind::Text,
    },
    Column {
        key: "models",
        label: "Models, comma-separated (empty = ask the server)",
        kind: Kind::List,
    },
    Column {
        key: "vision",
        label: "Accepts images",
        kind: Kind::Choice(&["auto", "yes", "no"]),
    },
];
