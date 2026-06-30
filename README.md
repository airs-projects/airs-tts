# airs-tts

This document lists the CLI and public API.

## CLI

- `airs-tts --help` - Show help.
- `airs-tts --version` - Show version.
- `airs-tts list_voices [--backend <name>]` - List available voices.
- `airs-tts pipe -i <source> -o <target> [-o <target>...] [--voice <name>] [--speed <value>] [--backend <name>]` - Synthesize text.

`-i text:<text>`, `-i file:<path>`, or `-i stdin` is required once. `-o file:<path>` or `-o device[:name]` is required at least once and may be repeated.

```
airs-tts pipe -i text:"Hello world" -o device
airs-tts pipe -i file:input.txt -o file:speech.wav
airs-tts pipe -i stdin -o device:speaker -o file:speech.wav
```

## Library public API

- `version()` - Return the crate version string.
- `Result<T>` - Library result type using `TtsError`.
- `TtsError` - Error enum for invalid input, backend load, synthesis, and audio failures.

- `Processor` - Text-to-speech processor with chainable backend configuration.
- `Processor::new()` - Create a new processor with default backend and voice.
- `TtsBackendKind` - Backend selection enum (e.g. `TtsBackendKind::Kokoro`).
- `Processor::set_backend(kind)` - Set the backend implementation.
- `Processor::set_voice(name)` - Set the voice by name (e.g. `"af_heart"`, `"bf_emma"`, `"zf_xiaobei"`).
- `Processor::set_speed(value)` - Set the speech speed multiplier (0.5-2.0, default 1.0).
- `Processor::init()` - Async; load the selected implementation before synthesis.
- `Processor::process(text)` - Async; one-shot synthesis: feed text and return a single `AudioFrame`.
- `Processor::is_ready()` - Return whether the selected backend has been initialized.
- `Processor::list_voices()` - List all available voice names.
