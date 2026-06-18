# airs-tts

This document lists the CLI and public API.

## CLI

- `airs-tts --help` - Show help.
- `airs-tts --version` - Show version.
- `airs-tts list_voices [--backend <name>]` - List available voices.
- `airs-tts pipe -i:t <text> -i:f <file> -i:s -o:d [device] -o:f <file> [--voice <name>] [--speed <value>] [--backend <name>]` - Synthesize text.

`-i:t`, `-i:f`, or `-i:s` is required once. `-o:d` or `-o:f` is required at least once and may be repeated.

```
airs-tts pipe -i:t "Hello world" -o:d
airs-tts pipe -i:f input.txt -o:f speech.wav
airs-tts pipe -i:s -o:d speaker -o:f speech.wav
```

## Library public API

- `version()` - Return the crate version string.
- `Result<T>` - Library result type using `TtsError`.
- `TextStream` - Boxed stream of sentence text using `TtsError`.
- `TtsError` - Error enum for invalid input, backend load, synthesis, and audio failures.

- `TextInputSource` - Input source enum: text, file, or stdin.
- `TextInput` - Self-builder for text, file, or stdin input. Consumed by `.open()` to produce a sentence `TextStream`.
- `TextInput::new(source)` - Select the text input source.
- `TextInput::open(self)` - Consume and build the sentence `TextStream`.

- `TtsEngine` - Text-to-speech engine with chainable backend configuration.
- `TtsEngine::new()` - Create a new engine with default backend and voice.
- `TtsBackendKind` - Backend selection enum (e.g. `TtsBackendKind::Kokoro`).
- `TtsEngine::backend(kind)` - Set the backend implementation.
- `TtsEngine::voice(name)` - Set the voice by name (e.g. `"af_heart"`, `"bf_emma"`, `"zf_xiaobei"`).
- `TtsEngine::speed(value)` - Set the speech speed multiplier (0.5-2.0, default 1.0).
- `TtsEngine::init()` - Load the selected implementation before the first synthesis call.
- `TtsEngine::is_ready()` - Return whether the selected backend has been initialized.
- `TtsEngine::list_voices()` - List all available voice names.
- `TtsEngine::invoke(text)` - Invoke the selected backend and return speech audio in a single `AudioSlice` (24kHz mono f32 PCM).

- `SentenceSplitter` - Incrementally split text chunks into complete sentences.
- `SentenceSplitter::new()` - Create an empty splitter.
- `SentenceSplitter::push(chunk)` - Append text and return complete sentences.
- `SentenceSplitter::finish()` - Return the remaining buffered text, if any.
