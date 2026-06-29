---
name: airs-tts
description: Text-to-speech synthesis.
---

- `airs-tts --help` - Show help.
- `airs-tts --version` - Show version.
- `airs-tts list_voices [--backend <name>]` - List available voices.
- `airs-tts pipe -i <source> -o <target> [-o <target>...] [--voice <name>] [--speed <value>] [--backend <name>]` - Synthesize text. `-i text:<text>`, `-i file:<path>`, or `-i stdin` is required once. `-o file:<path>` or `-o device[:name]` is required at least once and may be repeated. Defaults: voice=af_heart, speed=1.0, backend=kokoro.

Requires espeak-ng and the Kokoro ONNX model files.
