---
name: airs-tts
description: Text-to-speech synthesis.
---

- `airs-tts --help` - Show help.
- `airs-tts --version` - Show version.
- `airs-tts list_voices [--backend <name>]` - List available voices.
- `airs-tts pipe -i:t <text> -i:f <file> -i:s -o:d [device] -o:f <file> [--voice <name>] [--speed <value>] [--backend <name>]` - Synthesize text. `-i:t`, `-i:f`, or `-i:s` is required once. `-o:d` or `-o:f` is required at least once and may be repeated. Defaults: voice=af_heart, speed=1.0, backend=kokoro.

Requires espeak-ng and the Kokoro ONNX model files.
