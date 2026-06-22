use airs_audio::AudioSlice;
use futures::{Sink, Stream};

use crate::{Result, TtsError};

#[cfg(feature = "kokoro")]
pub(crate) mod kokoro;

pub(crate) trait TtsBackend:
    Sink<String, Error = TtsError> + Stream<Item = Result<AudioSlice>> + Send + Unpin
{
    fn init(&mut self) -> Result<()>;
    /// Set the active voice, loading any voice-specific resources.
    fn set_voice(&mut self, voice: &str) -> Result<()>;
    fn set_speed(&mut self, speed: f32);
    fn list_voices(&mut self) -> Result<Vec<String>>;
}
