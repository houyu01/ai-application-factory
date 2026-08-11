//! Streaming screenplay-installment checkpoints for the long-drama creation banner.

use std::{
    thread,
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    skills,
};

use super::super::retry::immediate_language_retry_delay;
use super::{
    support::{char_count, expansion_progress, join_screenplay},
    DurableWorker,
};

const PREVIEW_WRITE_INTERVAL: Duration = Duration::from_millis(250);
const PREVIEW_WRITE_MIN_BYTES: usize = 96;

impl DurableWorker {
    /// Streams one screenplay installment into the bootstrap task preview while retaining only generated body text.
    pub(super) fn stream_screenplay_installment(
        &self,
        task_id: &str,
        snapshot: &mut Map<String, Value>,
        completed_screenplay: &str,
        model: Option<&str>,
        prompt: &str,
        web: bool,
        stage: &str,
        target_chars: usize,
    ) -> AppResult<String> {
        let system = format!(
            "{}\n\n你是短剧长篇编剧，严格遵守用户项目的集数、字数、编号与原创要求。",
            skills::drama_agent_system()?
        );
        for attempt in 1..=3 {
            let mut installment = String::new();
            let mut saved_bytes = 0;
            let mut last_saved = Instant::now() - PREVIEW_WRITE_INTERVAL;
            let response = self.providers.complete_with_web_search_content_stream(
                "language",
                model,
                &system,
                prompt,
                web,
                |delta| {
                    installment.push_str(delta);
                    let due = installment.len().saturating_sub(saved_bytes)
                        >= PREVIEW_WRITE_MIN_BYTES
                        || last_saved.elapsed() >= PREVIEW_WRITE_INTERVAL;
                    if due {
                        self.persist_screenplay_preview(
                            task_id,
                            snapshot,
                            completed_screenplay,
                            &installment,
                            stage,
                            target_chars,
                        )?;
                        saved_bytes = installment.len();
                        last_saved = Instant::now();
                    }
                    Ok(())
                },
            );
            if saved_bytes != installment.len() {
                self.persist_screenplay_preview(
                    task_id,
                    snapshot,
                    completed_screenplay,
                    &installment,
                    stage,
                    target_chars,
                )?;
            }
            match response {
                Ok(Some(value)) => return Ok(value),
                Ok(None) => return Err(AppError::BadRequest("未配置可调用的语言模型".to_owned())),
                Err(error) if immediate_language_retry_delay(&error, attempt).is_some() => {
                    let delay = immediate_language_retry_delay(&error, attempt)
                        .expect("retry delay was checked");
                    self.repository.update_drama_task_progress(
                        task_id,
                        expansion_progress(char_count(completed_screenplay), target_chars),
                        &format!(
                            "{stage}连接短暂异常，{} 秒后立即重试（{}/3）",
                            delay.as_secs(),
                            attempt + 1
                        ),
                    )?;
                    thread::sleep(delay);
                }
                Err(error) => {
                    return Err(AppError::External(format!(
                        "{stage}请求语言模型失败（已尝试 {attempt} 次）：{error}"
                    )));
                }
            }
        }
        unreachable!()
    }

    fn persist_screenplay_preview(
        &self,
        task_id: &str,
        snapshot: &mut Map<String, Value>,
        completed_screenplay: &str,
        installment: &str,
        stage: &str,
        target_chars: usize,
    ) -> AppResult<()> {
        self.ensure_expansion_active(task_id)?;
        let preview = join_screenplay(completed_screenplay, installment);
        snapshot.insert("expanded_script_preview".to_owned(), json!(preview));
        self.repository
            .update_drama_task_snapshot(task_id, Value::Object(snapshot.clone()))?;
        self.repository.update_drama_task_progress(
            task_id,
            expansion_progress(char_count(&preview), target_chars),
            &format!("{stage}（已接收 {} 字）", char_count(installment)),
        )
    }
}
