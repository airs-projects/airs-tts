use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};

use airs_audio::AudioSlice;
pub use airs_io::{InputSource, OutputTarget, TextInput, TextSplitter, TextStream};
use futures::{Sink, Stream};

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
/// lightweight operation 鈥?it only reads file names, not the full voice data
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
        backend.set_speed(self.speed);
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
}

impl Sink<String> for TtsEngine {
    type Error = TtsError;

    fn poll_ready(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Result<()>> {
        if !self.is_ready {
            return Poll::Ready(Err(TtsError::InvalidInput(
                "TTS backend is not initialized. Call init() first.".to_string(),
            )));
        }

        let backend = self.backend.as_mut().expect("ready engine has backend");
        Pin::new(&mut **backend).poll_ready(context)
    }

    fn start_send(mut self: Pin<&mut Self>, item: String) -> Result<()> {
        if !self.is_ready {
            return Err(TtsError::InvalidInput(
                "TTS backend is not initialized. Call init() first.".to_string(),
            ));
        }

        let backend = self.backend.as_mut().expect("ready engine has backend");
        Pin::new(&mut **backend).start_send(item)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Result<()>> {
        if !self.is_ready {
            return Poll::Ready(Err(TtsError::InvalidInput(
                "TTS backend is not initialized. Call init() first.".to_string(),
            )));
        }

        let backend = self.backend.as_mut().expect("ready engine has backend");
        Pin::new(&mut **backend).poll_flush(context)
    }

    fn poll_close(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Result<()>> {
        if !self.is_ready {
            return Poll::Ready(Err(TtsError::InvalidInput(
                "TTS backend is not initialized. Call init() first.".to_string(),
            )));
        }

        let backend = self.backend.as_mut().expect("ready engine has backend");
        Pin::new(&mut **backend).poll_close(context)
    }
}

impl Stream for TtsEngine {
    type Item = Result<AudioSlice>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if !self.is_ready {
            return Poll::Ready(Some(Err(TtsError::InvalidInput(
                "TTS backend is not initialized. Call init() first.".to_string(),
            ))));
        }

        let backend = self.backend.as_mut().expect("ready engine has backend");
        Pin::new(&mut **backend).poll_next(context)
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
}
