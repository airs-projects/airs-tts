use std::collections::HashMap;
use std::path::Path;

use super::model::KokoroError;

/// Storage for all loaded voice style vectors.
///
/// Each voice is stored as a flat list of style vectors, where each vector
/// has 256 floats. The index into the list corresponds to the phoneme token
/// count, enabling prosody-consistent synthesis.
pub struct VoiceStore {
    voices: HashMap<String, Vec<[f32; 256]>>,
}

impl VoiceStore {
    /// Load all voices from a model directory.
    ///
    /// Expects the onnx-community layout: `voices/*.bin` raw float32 files
    /// where each file name is the voice name.
    pub fn load(model_dir: &Path) -> Result<Self, KokoroError> {
        let voices_dir = model_dir.join("voices");
        if voices_dir.is_dir() {
            return Self::load_raw_bin_dir(&voices_dir);
        }

        Err(KokoroError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "Voice files not found. Expected {}.",
                voices_dir.display()
            ),
        )))
    }

    /// Load all voices from onnx-community raw float32 `.bin` files.
    ///
    /// Each file name is the voice name, and each file contains a flat
    /// `[N, 1, 256]` or `[N, 256]` float32 array.
    fn load_raw_bin_dir(voices_dir: &Path) -> Result<Self, KokoroError> {
        let mut voices = HashMap::new();

        for entry in std::fs::read_dir(voices_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
                continue;
            }

            let voice_name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|stem| !stem.is_empty())
                .ok_or_else(|| {
                    KokoroError::VoiceParse(format!("Invalid voice file name: {}", path.display()))
                })?
                .to_string();

            let data = std::fs::read(&path).map_err(|e| {
                KokoroError::VoiceParse(format!("Failed to read {}: {e}", path.display()))
            })?;
            let style_vectors = parse_raw_f32_styles(&data, &voice_name)?;
            voices.insert(voice_name, style_vectors);
        }

        if voices.is_empty() {
            return Err(KokoroError::VoiceParse(format!(
                "No .bin voice files found in {}",
                voices_dir.display()
            )));
        }

        log::info!("Loaded {} voices", voices.len());
        Ok(Self { voices })
    }

    /// Get the style vector for a voice at the given index.
    ///
    /// The index is clamped to the valid range, so any index is safe.
    pub fn get_style(&self, voice: &str, idx: usize) -> Result<[f32; 256], KokoroError> {
        let styles = self
            .voices
            .get(voice)
            .ok_or_else(|| KokoroError::VoiceNotFound(voice.to_string()))?;

        let clamped = idx.min(styles.len().saturating_sub(1));
        Ok(styles[clamped])
    }

    /// List all available voice names in sorted order.
    pub fn list_voices(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.voices.keys().map(|s| s.as_str()).collect();
        names.sort_unstable();
        names
    }
}

/// Parse raw float32 style vectors from a .bin file.
///
/// Expects a flat `[N, 256]` float32 array in little-endian format.
fn parse_raw_f32_styles(data: &[u8], name: &str) -> Result<Vec<[f32; 256]>, KokoroError> {
    if !data.len().is_multiple_of(4) {
        return Err(KokoroError::VoiceParse(format!(
            "{name}: byte length {} is not a multiple of 4",
            data.len()
        )));
    }

    let n_floats = data.len() / 4;
    if !n_floats.is_multiple_of(256) {
        return Err(KokoroError::VoiceParse(format!(
            "{name}: float count {n_floats} is not a multiple of 256 (style vector dim)"
        )));
    }

    let n_styles = n_floats / 256;
    let mut result = Vec::with_capacity(n_styles);

    for i in 0..n_styles {
        let mut vec = [0f32; 256];
        for (j, slot) in vec.iter_mut().enumerate() {
            let offset = (i * 256 + j) * 4;
            *slot = f32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
        }
        result.push(vec);
    }

    Ok(result)
}

/// Scan the default model directory for available Kokoro voice names
/// without loading the full voice data or ONNX model.
///
/// Returns structured [`VoiceInfo`] entries with name, language, and backend.
pub(crate) fn list_available_voices() -> crate::Result<Vec<crate::VoiceInfo>> {
    let base = crate::model_path("kokoro");
    let voices_dir = base.join("voices");

    if !voices_dir.is_dir() {
        return Err(crate::TtsError::BackendLoad(format!(
            "Voice directory not found. Expected {}.",
            voices_dir.display()
        )));
    }

    let names = list_bin_voice_names(&voices_dir)?;

    Ok(names
        .into_iter()
        .map(|name| {
            let language = voice_language(&name).to_string();
            crate::VoiceInfo {
                name,
                language,
                backend: "kokoro".to_string(),
            }
        })
        .collect())
}

fn list_bin_voice_names(voices_dir: &std::path::Path) -> crate::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(voices_dir).map_err(crate::TtsError::Io)? {
        let entry = entry.map_err(crate::TtsError::Io)?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("bin") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if !stem.is_empty() {
                names.push(stem.to_string());
            }
        }
    }
    if names.is_empty() {
        return Err(crate::TtsError::BackendLoad(format!(
            "No .bin voice files found in {}",
            voices_dir.display()
        )));
    }
    Ok(names)
}

/// Map a Kokoro voice name prefix to a human-readable language label.
///
/// Voice names follow the pattern `{prefix}_{name}` where the two-character
/// prefix encodes the language.
fn voice_language(voice: &str) -> &'static str {
    let prefix = &voice[..voice.len().min(2)];
    match prefix {
        "af" | "am" => "English (US)",
        "bf" | "bm" => "English (GB)",
        "ef" | "em" => "Spanish",
        "ff" => "French",
        "hf" | "hm" => "Hindi",
        "if" | "im" => "Italian",
        "jf" | "jm" => "Japanese",
        "pf" | "pm" => "Portuguese (Brazil)",
        "zf" | "zm" => "Mandarin Chinese",
        _ => "English (US)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_model_dir(test_name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "airs-tts-{test_name}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create temp model dir");
        path
    }

    #[test]
    fn load_prefers_onnx_community_voice_bins() {
        let dir = temp_model_dir("community-voices");
        let voices_dir = dir.join("voices");
        std::fs::create_dir_all(&voices_dir).expect("create voices dir");

        let mut data = Vec::new();
        for value in 0..256 {
            data.extend_from_slice(&(value as f32).to_le_bytes());
        }
        std::fs::write(voices_dir.join("af_heart.bin"), data).expect("write voice bin");

        let store = VoiceStore::load(&dir).expect("load voices");

        assert_eq!(store.list_voices(), vec!["af_heart"]);
        let style = store.get_style("af_heart", 0).expect("get style");
        assert_eq!(style[0], 0.0);
        assert_eq!(style[255], 255.0);
        std::fs::remove_dir_all(dir).expect("remove temp model dir");
    }
}
