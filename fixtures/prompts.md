# Prompt and pattern vectors

Golden copies of the built-in prompts, and the input/output vectors the core
tests check against (crates/core/src/tasks/tests.rs, llm/tests.rs). Each
value ends where its closing fence starts: no trailing newline. A changed
prompt changes here too, on purpose.

## Chat with a selection

```text
You are a helpful assistant. The user is working on the text shown below. Answer their questions about it — explain, clarify, shorten, reword, translate, debug, or discuss it as they ask. Keep answers concise and in the same language as the user's message. Do not re-emit the whole text unless explicitly asked.

TEXT:
{text}
```

## Chat without a selection

```text
You are a helpful assistant. Answer the user's questions concisely, in the same language they use.
```

## Image rewrite

The prompt; `{task}` is one of the two blocks after it.

```text
The attached image(s) contain text — a screenshot, photo or scan. Read that text carefully. {task}
Output ONLY the resulting text, as plain text: no commentary, no notes, no headings, no options, and do not describe the image.
```

The task with an instruction:

```text
Then follow this instruction on it: "{instruction}".
```

The task without one:

```text
Transcribe it faithfully, silently fixing dictation/OCR artifacts, punctuation and obvious typos — nothing else.
```

Added before a selected text (JSON-escaped to show its newlines):

```json
"\n\nTreat this selected text as further input, along with the image(s):\n"
```

## Language check

```text
Answer with exactly one word: is the following text written in {lang}? Answer "yes" or "no".

TEXT:
{text}
```

## Translate

The instruction, then rendered with home German and other English: from
home, then to home.

```text
Translate the text from {src} into {dst}. Preserve the meaning, tone, formatting and level of formality. Output only the {dst} translation, nothing else.
```

```text
Translate the text from German into English. Preserve the meaning, tone, formatting and level of formality. Output only the English translation, nothing else.
```

```text
Translate the text from its original language into German. Preserve the meaning, tone, formatting and level of formality. Output only the German translation, nothing else.
```

## Model filter

Case-insensitive, searched anywhere in a model id; a hit hides the model.

```text
(^|[-_/@])(mtp|draft|drafter|dflash\d*)([-_/@.]|$)|assistant$|(tts|asr|whisper|parakeet|embed)|@\?$
```

Vectors `[id, offered]`:

```json
["gemma-4-12b-it", true]
["gemma-4-12b-it@?", false]
["gemma4-assistant", false]
["mtp-gemma-4-12b-it.gguf", false]
["gemma-4-12b-it-MTP", false]
["Qwen3.8-27B-DFlash2", false]
["qwen3.8-27b-dflash", false]
["qwen-dflash-drafter", false]
["mistral-small-draft", false]
["mistral-small-drafts", true]
["llama-3-8b@draft.gguf", false]
["some-assistant", false]
["assistant-v2", true]
["text-embedding-nomic-embed-text-v1.5", false]
["bge-m3-embedding", false]
["parakeet-tdt-0.6b-v3", false]
["whisper-large-v3", false]
["kokoro-tts", false]
["qwen3-asr-1.7b", false]
["Ornith-35B-A3B-OptiQ", true]
["Qwen3.8-27B-oQ4e-mtp", false]
["gemma-4-e4b-it-mlx@6bit", true]
["gpt-oss-20b", true]
["multiple-choice-7b", true]
["fasttsmodel", false]
["", true]
```

## German heuristic

Vectors `[text, looks German]`:

```json
["Das ist ein Test", true]
["Hallo, wie geht es dir?", false]
["Straße", true]
["Hello World über", true]
["The quick brown fox", false]
["I am in the office", true]
["Ich bin hier", true]
["was ist das", true]
["HALLO DAS IST GUT", true]
["Café au lait", false]
["12345", false]
["", false]
["Die Katze schläft", true]
["Wir sehen uns morgen", true]
["We will see you tomorrow at noon", false]
["Er kommt nicht", true]
["One two three four five six in", false]
```

The function words, sorted:

```text
aber als auch auf bei das dass dem den der des die du durch ein eine einem einen er für haben hat hatte hier ich ihr im in ist jetzt mehr mit nach nicht noch nur oder ohne schon sehr sein sich sie sind und unter von vor war waren weil wenn werden wir wird wurde zu über
```

## Unwrap

Vectors `[answer, unwrapped]`:

~~~~json
["  plain answer  ", "plain answer"]
["```\ncode\n```", "code"]
["```python\nprint(1)\n```", "print(1)"]
["```text\nHello there\n```  ", "Hello there"]
["```\nno closing fence", "no closing fence"]
["```json{\"a\":1}```", "{\"a\":1}"]
["```c++\nx\n```", "x"]
["\"quoted\"", "quoted"]
["'single'", "single"]
["“curly”", "“curly”"]
["“same“", "same"]
["\"  padded  \"", "padded"]
["\"", "\""]
["\"a\"b\"", "a\"b"]
["```\n\"both\"\n```", "both"]
["'", "'"]
["", ""]
["It's fine", "It's fine"]
["\"unbalanced", "\"unbalanced"]
["```\n```", ""]
~~~~
