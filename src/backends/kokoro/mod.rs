#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::backends::TtsBackend;
use airs_audio::AudioFrame;
use async_trait::async_trait;
use engine::{KokoroInferenceParams, KokoroModelParams};
use futures::{Sink, Stream};

use crate::{Result, TtsError, model_path};

mod engine;
mod model;
mod phonemizer;
mod vocab;
pub(crate) mod voices;

pub(crate) use engine::KokoroEngine;

#[async_trait]
impl TtsBackend for KokoroEngine {
    async fn init(&mut self) -> Result<()> {
        if self.is_loaded() {
            return Ok(());
        }

        let model_path = self.model_path().unwrap_or_else(|| model_path("kokoro"));
        let onnx_path =
            model::find_onnx_file(&model_path).map_err(|e| TtsError::BackendLoad(e.to_string()))?;
        let optimized_model_path = optimized_model_path(&onnx_path);
        if let Some(parent) = optimized_model_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| TtsError::BackendLoad(e.to_string()))?;
        }

        self.load_model_with_params(
            &model_path,
            KokoroModelParams {
                num_threads: None,
                optimized_model_cache_path: Some(optimized_model_path),
            },
        )
        .map_err(|e| TtsError::BackendLoad(e.to_string()))
    }

    fn set_voice(&mut self, voice: &str) -> Result<()> {
        KokoroEngine::set_voice(self, voice).map_err(|e| TtsError::BackendLoad(e.to_string()))
    }

    fn set_speed(&mut self, speed: f32) {
        self.speed = speed;
    }

    fn list_voices(&mut self) -> Result<Vec<String>> {
        Ok(KokoroEngine::list_voices(self)
            .into_iter()
            .map(|s| s.to_string())
            .collect())
    }

    async fn process(&mut self, text: String) -> Result<AudioFrame> {
        KokoroEngine::process(self, text).map_err(|e| TtsError::Synthesis(e.to_string()))
    }
}

impl Sink<String> for KokoroEngine {
    type Error = TtsError;

    fn poll_ready(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(mut self: Pin<&mut Self>, text: String) -> Result<()> {
        let params = KokoroInferenceParams {
            voice: self.voice.clone(),
            speed: self.speed,
            style_index: None,
        };

        let result = self
            .synthesize(&text, Some(params))
            .map_err(|e| TtsError::Synthesis(e.to_string()))?;

        self.pending.push_back(Ok(AudioFrame {
            samples: result.samples,
            channels: 1,
            sample_rate: result.sample_rate,
        }));
        self.closed = false;
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Result<()>> {
        self.closed = true;
        Poll::Ready(Ok(()))
    }
}

impl Stream for KokoroEngine {
    type Item = Result<AudioFrame>;

    fn poll_next(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(slice) = self.pending.pop_front() {
            return Poll::Ready(Some(slice));
        }

        if self.closed {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}

fn optimized_model_path(onnx_path: &Path) -> PathBuf {
    let name = onnx_path
        .file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("model");
    onnx_path.with_file_name(format!("{name}.optimized.onnx"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimized_model_path_uses_onnx_stem() {
        let path = optimized_model_path(&model_path("kokoro").join("model_q8f16.onnx"));

        assert!(path.ends_with(".airs/models/kokoro/model_q8f16.optimized.onnx"));
    }
}
