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
- `TtsError` - Error enum for invalid input, backend load, synthesis, and audio failures.

- `InputSource` - Re-export from `airs-io`; input source enum shared by text/audio/ASR/TTS crates.
- `OutputTarget` - Re-export from `airs-io`; output target enum shared by text/audio/ASR/TTS crates.
- `TextInput` - Re-export from `airs-io`; implements `Stream<Item = airs_io::Result<String>>`.

- `TtsEngine` - Text-to-speech engine with chainable backend configuration.
- `TtsEngine::new()` - Create a new engine with default backend and voice.
- `TtsBackendKind` - Backend selection enum (e.g. `TtsBackendKind::Kokoro`).
- `TtsEngine::set_backend(kind)` - Set the backend implementation.
- `TtsEngine::set_voice(name)` - Set the voice by name (e.g. `"af_heart"`, `"bf_emma"`, `"zf_xiaobei"`).
- `TtsEngine::set_speed(value)` - Set the speech speed multiplier (0.5-2.0, default 1.0).
- `TtsEngine::init()` - Async; load the selected implementation before synthesis.
- `TtsEngine::call(text)` - Async; one-shot synthesis: feed text and return a single `AudioSlice`.
- `TtsEngine::is_ready()` - Return whether the selected backend has been initialized.
- `TtsEngine::list_voices()` - List all available voice names.

- `TextSplitter` - Incrementally split text chunks into complete sentences.
- `TextSplitter::new()` - Create an empty splitter.
- `TextSplitter::push(chunk)` - Append text and return complete sentences.
- `TextSplitter::finish()` - Return the remaining buffered text, if any.