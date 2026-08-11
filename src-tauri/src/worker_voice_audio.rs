//! Durable configured-audio-model execution for system and custom catalog voice samples.

use serde_json::Value;

use crate::error::AppResult;

use super::DurableWorker;

impl DurableWorker {
    /// Generate one persisted sample sentence, attach it to a confirmed catalog voice when applicable, and retain a retryable error otherwise.
    pub(super) fn run_voice_audio(&self, task: Value) {
        let task_id = task["id"].as_str().unwrap_or_default();
        let result = self
            .providers
            .synthesize_voice_sample(
                task["sample_text"].as_str().unwrap_or_default(),
                task["voice_id"].as_str(),
                task["name"].as_str().unwrap_or_default(),
                task["gender"].as_str().unwrap_or_default(),
                task["prompt"].as_str().unwrap_or_default(),
            )
            .and_then(|url| self.finish_voice_audio(task_id, &url));
        if let Err(error) = result {
            let _ = self
                .repository
                .fail_voice_audio_task(task_id, &error.to_string());
        }
    }

    fn finish_voice_audio(&self, task_id: &str, audio_url: &str) -> AppResult<()> {
        let previous = self
            .repository
            .finish_voice_audio_task(task_id, audio_url)?;
        if let Some(previous) = previous {
            self.media.delete_url(Some(&previous))?;
        }
        Ok(())
    }
}
