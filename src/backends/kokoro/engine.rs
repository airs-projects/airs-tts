use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use airs_audio::AudioFrame;

use super::model::{KokoroError, KokoroModel, SAMPLE_RATE};
use super::phonemizer::EspeakConfig;

#[derive(Debug)]
pub(super) struct SynthesisResult {
    pub(super) samples: Vec<f32>,
    pub(super) sample_rate: u32,
}

/// Parameters for configuring Kokoro model loading.
#[derive(Debug, Clone, Default)]
pub struct KokoroModelParams {
    /// Number of CPU threads to use for inference.
    /// `None` uses the ORT default (typically all available cores).
    pub num_threads: Option<usize>,
    /// Path for caching the Level3-optimized ONNX graph.
    ///
    /// - First load: ORT runs Level3 optimization and serialises the result here.
    /// - Subsequent loads: the pre-built graph is loaded at `Disable` optimization,
    ///   skipping the expensive 5–10 s re-optimization step entirely.
    ///
    /// Always write to a writable location (e.g. app data dir); bundled resource
    /// directories may be read-only.
    pub optimized_model_cache_path: Option<PathBuf>,
}

/// Parameters for configuring a Kokoro synthesis request.
#[derive(Debug, Clone)]
pub struct KokoroInferenceParams {
    /// Voice name (e.g. `"af_heart"`, `"bf_emma"`, `"jf_alpha"`).
    pub voice: String,
    /// Speech speed multiplier. Range: 0.5–2.0, default 1.0.
    pub speed: f32,
    /// Override the style vector index. `None` = auto (uses phoneme token count).
    pub style_index: Option<usize>,
}

impl Default for KokoroInferenceParams {
    fn default() -> Self {
        Self {
            voice: "af_heart".to_string(),
            speed: 1.0,
            style_index: None,
        }
    }
}

/// Kokoro text-to-speech engine.
///
/// Uses the Kokoro-82M ONNX model for high-quality, fast TTS with support
/// for 9 languages. Requires espeak-ng for phonemization.
///
/// # Quick Start
///
/// ```ignore
/// use airs_tts::backends::kokoro::KokoroEngine;
/// use std::path::PathBuf;
///
/// // Uses system espeak-ng from PATH
/// let mut engine = KokoroEngine::new();
/// engine.load_model_with_params(&PathBuf::from("models/kokoro"), Default::default())?;
/// let result = engine.synthesize("Hello, world!", None)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Bundled espeak-ng
///
/// ```ignore
/// use airs_tts::backends::kokoro::KokoroEngine;
/// use std::path::PathBuf;
///
/// // Point to a bundled espeak-ng binary and data directory
/// let engine = KokoroEngine::with_espeak(
///     Some(PathBuf::from("/app/resources/espeak-ng/espeak-ng")),
///     Some(PathBuf::from("/app/resources/espeak-ng-data")),
/// );
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct KokoroEngine {
    model: Option<KokoroModel>,
    model_path: Option<PathBuf>,
    espeak: EspeakConfig,
    pub(super) voice: String,
    pub(super) speed: f32,
    pub(super) pending: VecDeque<crate::Result<AudioFrame>>,
    pub(super) closed: bool,
}

impl Default for KokoroEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl KokoroEngine {
    /// Create a new engine that uses `espeak-ng` from PATH.
    pub fn new() -> Self {
        Self {
            model: None,
            model_path: None,
            espeak: EspeakConfig::default(),
            voice: "af_heart".to_string(),
            speed: 1.0,
            pending: VecDeque::new(),
            closed: false,
        }
    }

    /// Create a new engine with explicit espeak-ng binary and data paths.
    ///
    /// Use this when bundling espeak-ng with your application. Either path
    /// can be `None` to fall back to the system default.
    pub fn with_espeak(bin_path: Option<PathBuf>, data_path: Option<PathBuf>) -> Self {
        Self {
            model: None,
            model_path: None,
            espeak: EspeakConfig {
                bin_path,
                data_path,
            },
            voice: "af_heart".to_string(),
            speed: 1.0,
            pending: VecDeque::new(),
            closed: false,
        }
    }

    /// Set the active voice. Validates that the voice exists in the loaded model.
    pub fn set_voice(&mut self, voice: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(model) = self.model.as_ref() {
            let available = model.list_voices();
            if !available.contains(&voice) {
                return Err(Box::new(KokoroError::VoiceNotFound(voice.to_string())));
            }
        }
        self.voice = voice.to_string();
        Ok(())
    }

    pub(super) fn is_loaded(&self) -> bool {
        self.model.is_some()
    }

    pub(super) fn model_path(&self) -> Option<PathBuf> {
        self.model_path.clone()
    }

    pub(super) fn load_model_with_params(
        &mut self,
        model_path: &Path,
        params: KokoroModelParams,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let model = KokoroModel::load(
            model_path,
            params.num_threads,
            params.optimized_model_cache_path.as_deref(),
        )?;
        self.model = Some(model);
        self.model_path = Some(model_path.to_path_buf());
        Ok(())
    }

    pub(super) fn unload_model(&mut self) {
        self.model = None;
        self.model_path = None;
    }

    pub(super) fn synthesize(
        &mut self,
        text: &str,
        params: Option<KokoroInferenceParams>,
    ) -> Result<SynthesisResult, Box<dyn std::error::Error>> {
        let model = self.model.as_mut().ok_or(KokoroError::ModelNotLoaded)?;

        let p = params.unwrap_or_else(|| KokoroInferenceParams {
            voice: self.voice.clone(),
            speed: 1.0,
            style_index: None,
        });
        let samples =
            model.synthesize_text(text, &p.voice, p.speed, p.style_index, &self.espeak)?;

        Ok(SynthesisResult {
            samples,
            sample_rate: SAMPLE_RATE,
        })
    }

    /// List all available voice names (requires model to be loaded).
    pub fn list_voices(&self) -> Vec<&str> {
        self.model
            .as_ref()
            .map(|m| m.list_voices())
            .unwrap_or_default()
    }

    /// One-shot synthesis: feed text and return an audio frame synchronously.
    pub fn call(&mut self, text: String) -> Result<AudioFrame, Box<dyn std::error::Error>> {
        let params = KokoroInferenceParams {
            voice: self.voice.clone(),
            speed: self.speed,
            style_index: None,
        };
        let result = self.synthesize(&text, Some(params))?;
        Ok(AudioFrame {
            samples: result.samples,
            channels: 1,
            sample_rate: result.sample_rate,
        })
    }
}

impl Drop for KokoroEngine {
    fn drop(&mut self) {
        self.unload_model();
    }
}
