//! Fast, bounded recovery policy for language-model work owned by durable drama tasks.

use std::time::Duration;

use serde_json::Value;

use crate::error::AppError;

use super::DurableWorker;

const IMMEDIATE_LANGUAGE_ATTEMPTS: i64 = 3;
const DURABLE_LANGUAGE_ATTEMPTS: i64 = 4;
const DURABLE_RETRY_DELAYS: [i64; 3] = [1, 2, 5];

impl DurableWorker {
    /// Requeue only transient language-provider failures after fast in-call retries are exhausted.
    pub(super) fn retry_durable_provider_error(
        &self,
        task: &Value,
        id: &str,
        error: &AppError,
    ) -> bool {
        let AppError::External(message) = error else {
            return false;
        };
        let kind = task["type"].as_str().unwrap_or_default();
        let attempts = task["poll_attempts"].as_i64().unwrap_or(1);
        if ["script_decomposition", "script_expansion"].contains(&kind)
            && retryable_language_message(message)
            && attempts < DURABLE_LANGUAGE_ATTEMPTS
        {
            let delay = durable_retry_delay(attempts);
            return self
                .repository
                .reschedule_drama_task(
                    id,
                    delay,
                    &format!(
                        "语言模型短暂异常，{delay} 秒后自动重试（{attempts}/{DURABLE_LANGUAGE_ATTEMPTS}）"
                    ),
                    Some(message),
                )
                .is_ok();
        }
        let is_video_poll = kind == "shot_video"
            && task["provider_task_id"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
            && message.contains("请求失败");
        if is_video_poll {
            return self
                .repository
                .reschedule_drama_task(id, 10, "视频模型连接暂时不可用，10 秒后重试", Some(message))
                .is_ok();
        }
        false
    }
}

/// Return the short wait before retrying a single interrupted language request in the same worker run.
pub(super) fn immediate_language_retry_delay(error: &AppError, attempt: i64) -> Option<Duration> {
    (attempt < IMMEDIATE_LANGUAGE_ATTEMPTS && retryable_language_error(error))
        .then(|| Duration::from_secs(attempt.clamp(1, 2) as u64))
}

/// Identify failures caused by an unstable provider or incomplete stream, never malformed requests or bad credentials.
pub(super) fn retryable_language_error(error: &AppError) -> bool {
    matches!(error, AppError::External(message) if retryable_language_message(message))
}

fn durable_retry_delay(attempt: i64) -> i64 {
    DURABLE_RETRY_DELAYS[(attempt - 1).clamp(0, 2) as usize]
}

fn retryable_language_message(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    let terminal = [
        "api key",
        "权限",
        "所选模型",
        "模型名称",
        "endpoint",
        "请求参数",
        "安全审核",
        "内容未通过",
        "不支持",
        "额度不足",
        "invalid api",
        "unauthorized",
        "forbidden",
        "unsupported",
        "invalid parameter",
        "bad request",
    ];
    if terminal.iter().any(|needle| normalized.contains(needle)) {
        return false;
    }
    [
        "请求失败",
        "连接",
        "超时",
        "timeout",
        "timed out",
        "暂时不可用",
        "service unavailable",
        "响应读取失败",
        "没有返回文本结果",
        "incomplete message",
        "connection",
        "broken pipe",
        "reset by peer",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::{json, Map};

    use crate::{
        db::Database,
        error::AppError,
        media::MediaStore,
        repository::Repository,
        value::{new_id, GENERATING},
    };

    use super::{immediate_language_retry_delay, retryable_language_error, DurableWorker};

    #[test]
    fn retries_short_lived_transport_and_stream_failures() {
        for message in [
            "语言模型请求失败：服务商暂时不可用，请稍后重试。",
            "语言模型流式响应读取失败。原始错误：connection reset by peer",
            "故事圣经生成超时（3 分钟内未收到完整结果）",
            "语言模型没有返回文本结果",
        ] {
            assert!(retryable_language_error(&AppError::External(
                message.to_owned()
            )));
        }
        assert_eq!(
            immediate_language_retry_delay(
                &AppError::External("语言模型请求失败：服务商暂时不可用".to_owned()),
                1,
            )
            .expect("first retry")
            .as_secs(),
            1
        );
    }

    #[test]
    fn fails_fast_for_configuration_and_content_errors() {
        for message in [
            "API Key 无效或已失效，请检查模型配置中的 API Key。",
            "当前模型不支持此功能，请在设置中更换支持该功能的模型后重试。",
            "请求参数不符合服务商要求，请检查模型、提示词和参考素材后重试。",
        ] {
            assert!(!retryable_language_error(&AppError::External(
                message.to_owned()
            )));
        }
    }

    #[test]
    fn reschedules_a_transient_decomposition_failure_without_marking_it_failed() {
        let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
        let repository = Repository::new(
            Database::open(root.join("ai_application_factory.db")).expect("test database"),
        );
        let project = repository
            .create_drama(Map::from_iter([
                ("name".to_owned(), json!("自动重试短剧")),
                (
                    "script".to_owned(),
                    json!("主角在雨夜收到一封旧信，决定查清它的来历。"),
                ),
            ]))
            .expect("project");
        let project_id = project["id"].as_str().expect("project id");
        let task = repository
            .create_active_drama_task(project_id, "script_decomposition", None, json!({}))
            .expect("task");
        let task_id = task["id"].as_str().expect("task id");
        let claimed = repository
            .claim_drama_task_types(&["script_decomposition"])
            .expect("claim task")
            .expect("queued task");
        let worker = DurableWorker::new(
            repository.clone(),
            MediaStore::new(repository.clone()).expect("media store"),
        )
        .expect("worker");

        assert!(worker.retry_durable_provider_error(
            &claimed,
            task_id,
            &AppError::External("故事圣经生成超时（3 分钟内未收到完整结果）".to_owned()),
        ));

        let saved = repository
            .get_drama_task(task_id)
            .expect("rescheduled task");
        assert_eq!(saved["status"], GENERATING);
        assert!(saved["stage"]
            .as_str()
            .is_some_and(|stage| stage.contains("1 秒后自动重试")));
        assert!(saved["next_poll_at"].as_str().is_some());
        fs::remove_dir_all(root).expect("remove test database");
    }
}
