use airs_audio::AudioSlice;

use crate::Result;

#[cfg(feature = "kokoro")]
pub(crate) mod kokoro;

pub(crate) trait TtsBackend {
    fn init(&mut self) -> Result<()>;
    /// Set the active voice, loading any voice-specific resources.
    fn set_voice(&mut self, voice: &str) -> Result<()>;
    fn list_voices(&mut self) -> Result<Vec<String>>;
    fn invoke(&mut self, text: &str, speed: f32) -> Result<AudioSlice>;
}
