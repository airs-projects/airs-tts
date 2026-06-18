use std::collections::VecDeque;
use std::io::BufRead;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};

use airs_audio::AudioSlice;
use futures::channel::mpsc;
use tokio_stream::Stream;

mod backends;

use backends::TtsBackend;

#[derive(Debug, thiserror::Error)]
pub enum TtsError {
    #[error("{0}")]
    InvalidInput(String),
    #[error("failed to load TTS backend: {0}")]
    BackendLoad(String),
    #[error("TTS synthesis failed: {0}")]
    Synthesis(String),
    #[error("audio error: {0}")]
    Audio(#[from] airs_audio::AudioError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, TtsError>;
pub type TextStream = Pin<Box<dyn tokio_stream::Stream<Item = Result<String>> + Send>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TtsBackendKind {
    Kokoro,
}

impl Default for TtsBackendKind {
    fn default() -> Self {
        #[cfg(feature = "kokoro")]
        {
            Self::Kokoro
        }

        #[cfg(not(feature = "kokoro"))]
        {
            Self::Kokoro
        }
    }
}

/// Incrementally split text chunks into complete sentences.
#[derive(Debug, Default)]
pub struct SentenceSplitter {
    buffer: String,
}

impl SentenceSplitter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        self.buffer.push_str(chunk);

        let mut sentences = Vec::new();
        while let Some(end) = sentence_end(&self.buffer) {
            let sentence = self.buffer[..end].trim();
            if !sentence.is_empty() {
                sentences.push(sentence.to_string());
            }
            self.buffer.drain(..end);
        }

        sentences
    }

    pub fn finish(&mut self) -> Option<String> {
        let sentence = self.buffer.trim();
        if sentence.is_empty() {
            self.buffer.clear();
            None
        } else {
            let sentence = sentence.to_string();
            self.buffer.clear();
            Some(sentence)
        }
    }
}

fn sentence_end(text: &str) -> Option<usize> {
    for (index, ch) in text.char_indices() {
        if is_sentence_terminal(ch) && !is_decimal_point(text, index) {
            return Some(index + ch.len_utf8());
        }
    }

    None
}

fn is_sentence_terminal(ch: char) -> bool {
    matches!(
        ch,
        '.' | '!' | '?' | ';' | ':' | '\n' | '。' | '！' | '？' | '；' | '：'
    )
}

fn is_decimal_point(text: &str, index: usize) -> bool {
    let Some('.') = text[index..].chars().next() else {
        return false;
    };

    let previous = text[..index].chars().next_back();
    let next = text[index + '.'.len_utf8()..].chars().next();

    previous.is_some_and(|ch| ch.is_ascii_digit()) && next.is_some_and(|ch| ch.is_ascii_digit())
}

pub struct TextInput {
    source: TextInputSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextInputSource {
    Text(String),
    File(PathBuf),
    Stdin,
}

struct SentenceTextStream {
    chunks: TextStream,
    splitter: SentenceSplitter,
    pending: VecDeque<String>,
    done: bool,
}

impl TextInput {
    pub fn new(source: TextInputSource) -> Self {
        Self { source }
    }

    pub fn open(self) -> TextStream {
        let chunks = match self.source {
            TextInputSource::Text(text) => Box::pin(tokio_stream::iter(vec![Ok(text)])),
            TextInputSource::File(input) => Box::pin(tokio_stream::iter(vec![
                std::fs::read_to_string(input).map_err(TtsError::from),
            ])),
            TextInputSource::Stdin => stdin_chunks(),
        };

        Box::pin(SentenceTextStream {
            chunks,
            splitter: SentenceSplitter::new(),
            pending: VecDeque::new(),
            done: false,
        })
    }
}

fn stdin_chunks() -> TextStream {
    let (sender, receiver) = mpsc::unbounded::<Result<String>>();
    tokio::task::spawn_blocking(move || {
        for line in std::io::stdin().lock().lines() {
            let chunk = line.map(|line| format!("{line}\n")).map_err(TtsError::from);
            if sender.unbounded_send(chunk).is_err() {
                break;
            }
        }
    });

    Box::pin(receiver)
}

impl Stream for SentenceTextStream {
    type Item = Result<String>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if let Some(sentence) = self.pending.pop_front() {
                return Poll::Ready(Some(Ok(sentence)));
            }

            if self.done {
                return Poll::Ready(None);
            }

            match self.chunks.as_mut().poll_next(context) {
                Poll::Ready(Some(Ok(chunk))) => {
                    let sentences = self.splitter.push(&chunk);
                    self.pending.extend(sentences);
                }
                Poll::Ready(Some(Err(error))) => return Poll::Ready(Some(Err(error))),
                Poll::Ready(None) => {
                    if let Some(sentence) = self.splitter.finish() {
                        self.pending.push_back(sentence);
                    }
                    self.done = true;
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Information about an available TTS voice.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceInfo {
    /// Voice name (e.g., `"af_heart"`).
    pub name: String,
    /// Human-readable language name (e.g., `"English (US)"`).
    pub language: String,
    /// Backend that provides this voice (e.g., `"kokoro"`).
    pub backend: String,
}

impl std::fmt::Display for VoiceInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}  {:20}  [{}]", self.name, self.language, self.backend)
    }
}

/// List available voices without initialising the TTS engine.
///
/// Scans the default model directory for every compiled-in backend. This is a
/// lightweight operation — it only reads file names, not the full voice data
/// or ONNX model.
pub fn tts_list_voices() -> Result<Vec<VoiceInfo>> {
    let mut voices: Vec<VoiceInfo> = Vec::new();

    #[cfg(feature = "kokoro")]
    {
        match backends::kokoro::voices::list_available_voices() {
            Ok(kokoro) => voices.extend(kokoro),
            Err(e) => return Err(e),
        }
    }

    // Future backends append their voices here.

    voices.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(voices)
}

/// Text-to-speech engine with chainable backend configuration.
pub struct TtsEngine {
    backend_kind: TtsBackendKind,
    backend: Option<Box<dyn TtsBackend>>,
    is_ready: bool,
    voice: String,
    speed: f32,
}

impl Default for TtsEngine {
    fn default() -> Self {
        Self {
            backend_kind: TtsBackendKind::default(),
            backend: None,
            is_ready: false,
            voice: "af_heart".to_string(),
            speed: 1.0,
        }
    }
}

impl TtsEngine {
    /// Create a new TTS engine with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the backend implementation.
    pub fn backend(mut self, kind: TtsBackendKind) -> Self {
        self.backend_kind = kind;
        self.backend = None;
        self.is_ready = false;
        self
    }

    /// Set the voice by name.
    ///
    /// Validation happens at synthesis time when the engine looks up the voice.
    pub fn voice(mut self, voice: impl Into<String>) -> Self {
        self.voice = voice.into();
        self
    }

    /// Set the speech speed multiplier.
    pub fn speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    /// Load the selected implementation before the first synthesis call.
    pub fn init(mut self) -> Result<Self> {
        if self.backend.is_none() {
            #[cfg(feature = "kokoro")]
            {
                self.backend = Some(match self.backend_kind {
                    TtsBackendKind::Kokoro => Box::new(backends::kokoro::KokoroEngine::new()),
                });
            }

            #[cfg(not(feature = "kokoro"))]
            {
                match self.backend_kind {
                    TtsBackendKind::Kokoro => {
                        return Err(TtsError::InvalidInput(
                            "no TTS backend feature is enabled".to_string(),
                        ));
                    }
                }
            }
        }

        let backend = self.backend.as_mut().unwrap();
        backend.init()?;
        backend.set_voice(&self.voice)?;
        self.is_ready = true;
        Ok(self)
    }

    /// Return whether the selected backend has been initialized.
    pub fn is_ready(&self) -> bool {
        self.is_ready
    }

    /// List all available voice names.
    pub fn list_voices(&mut self) -> Result<Vec<String>> {
        if !self.is_ready {
            return Err(TtsError::InvalidInput(
                "TTS backend is not initialized. Call init() first.".to_string(),
            ));
        }

        self.backend.as_mut().unwrap().list_voices()
    }

    /// Invoke the selected backend and return speech audio in a single `AudioSlice`.
    pub fn invoke(&mut self, text: &str) -> Result<AudioSlice> {
        if !self.is_ready {
            return Err(TtsError::InvalidInput(
                "TTS backend is not initialized. Call init() first.".to_string(),
            ));
        }

        self.backend.as_mut().unwrap().invoke(text, self.speed)
    }
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(feature = "kokoro")]
fn model_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".airs/models")
}

#[cfg(feature = "kokoro")]
fn model_path(name: &str) -> PathBuf {
    model_dir().join(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[test]
    fn version_returns_expected() {
        assert_eq!(version(), "0.1.0");
    }

    #[test]
    fn engine_uses_default_settings() {
        let engine = TtsEngine::new();

        #[cfg(feature = "kokoro")]
        assert!(matches!(&engine.backend_kind, TtsBackendKind::Kokoro));
        assert!(engine.backend.is_none());
        assert!(!engine.is_ready());
        assert_eq!(engine.voice, "af_heart");
        assert_eq!(engine.speed, 1.0);
    }

    #[test]
    fn engine_supports_chainable_settings() {
        let engine = TtsEngine::new()
            .backend(TtsBackendKind::Kokoro)
            .voice("bf_emma")
            .speed(1.25);

        #[cfg(feature = "kokoro")]
        assert!(matches!(&engine.backend_kind, TtsBackendKind::Kokoro));
        assert!(engine.backend.is_none());
        assert!(!engine.is_ready());
        assert_eq!(engine.voice, "bf_emma");
        assert_eq!(engine.speed, 1.25);
    }

    #[test]
    fn engine_requires_init_before_use() {
        let mut engine = TtsEngine::new();

        let err = engine
            .list_voices()
            .expect_err("uninitialized engine should fail");

        assert!(matches!(err, TtsError::InvalidInput(_)));
        assert!(!engine.is_ready());
    }

    #[test]
    fn sentence_splitter_splits_complete_sentences() {
        let mut splitter = SentenceSplitter::new();

        let sentences = splitter.push("Hello world. Testing!");

        assert_eq!(sentences, vec!["Hello world.", "Testing!"]);
        assert_eq!(splitter.finish(), None);
    }

    #[test]
    fn sentence_splitter_keeps_incomplete_chunk() {
        let mut splitter = SentenceSplitter::new();

        assert!(splitter.push("Hello").is_empty());
        assert_eq!(splitter.push(" world."), vec!["Hello world."]);
        assert_eq!(splitter.finish(), None);
    }

    #[test]
    fn sentence_splitter_flushes_remaining_text() {
        let mut splitter = SentenceSplitter::new();

        assert!(splitter.push("No terminal punctuation").is_empty());

        assert_eq!(
            splitter.finish(),
            Some("No terminal punctuation".to_string())
        );
    }

    #[test]
    fn sentence_splitter_supports_cjk_punctuation() {
        let mut splitter = SentenceSplitter::new();

        let sentences = splitter.push("你好世界。继续吗？");

        assert_eq!(sentences, vec!["你好世界。", "继续吗？"]);
    }

    #[test]
    fn sentence_splitter_keeps_decimal_points() {
        let mut splitter = SentenceSplitter::new();

        let sentences = splitter.push("Version 2.0 is ready. Next.");

        assert_eq!(sentences, vec!["Version 2.0 is ready.", "Next."]);
    }

    #[tokio::test]
    async fn text_input_text_open_returns_sentence_stream() {
        let mut stream =
            TextInput::new(TextInputSource::Text("Hello world. Testing".to_string())).open();

        assert_eq!(stream.next().await.unwrap().unwrap(), "Hello world.");
        assert_eq!(stream.next().await.unwrap().unwrap(), "Testing");
        assert!(stream.next().await.is_none());
    }

    #[test]
    fn sentence_splitter_splits_across_chunks() {
        let mut splitter = SentenceSplitter::new();

        assert!(splitter.push("Hello").is_empty());
        assert_eq!(splitter.push(" world. Next"), vec!["Hello world."]);
        assert_eq!(splitter.push(" sentence."), vec!["Next sentence."]);
        assert_eq!(splitter.finish(), None);
    }
}
