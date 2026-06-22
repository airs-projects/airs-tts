use async_trait::async_trait;
use airs_audio::AudioSlice;
use futures::{Sink, Stream};

use crate::{Result, TtsError};

#[cfg(feature = "kokoro")]
pub(crate) mod kokoro;

#[async_trait]
pub(crate) trait TtsBackend:
    Sink<String, Error = TtsError> + Stream<Item = Result<AudioSlice>> + Send + Unpin
{
    async fn init(&mut self) -> Result<()>;
    fn set_voice(&mut self, voice: &str) -> Result<()>;
    fn set_speed(&mut self, speed: f32);
    fn list_voices(&mut self) -> Result<Vec<String>>;
    async fn call(&mut self, text: String) -> Result<AudioSlice>;
}
