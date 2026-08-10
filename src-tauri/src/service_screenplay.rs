//! Screenplay-dialog state, idempotent continuation enqueueing, and latest-task cancellation.

use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    value::{CANCELLED, FAILED, GENERATING},
};

use super::DesktopService;

impl DesktopService {
    /// Return the dialog payload enriched with the durable expansion task fields expected by the existing frontend.
    pub fn expanded_screenplay(&self, project_id: &str) -> AppResult<Value> {
        let mut screenplay = self.repository.get_expanded_screenplay(project_id)?;
        let latest = self.repository.latest_expansion_task(project_id)?;
        let active = latest
            .as_ref()
            .filter(|task| task["status"].as_str() == Some(GENERATING));
        let stage = latest
            .as_ref()
            .and_then(|task| task["stage"].as_str())
            .unwrap_or_default();
        let object = screenplay
            .as_object_mut()
            .expect("screenplay must be an object");
        object.insert(
            "expanded_script_generating".to_owned(),
            json!(active.is_some()),
        );
        object.insert(
            "expanded_script_cancellable".to_owned(),
            json!(active.is_some()),
        );
        object.insert(
            "expanded_script_cancel_label".to_owned(),
            json!(if active.is_some()
                && latest.as_ref().and_then(|task| task["type"].as_str())
                    == Some("script_decomposition")
                && stage.contains("拆解")
            {
                "取消生成"
            } else {
                "取消扩写"
            }),
        );
        object.insert(
            "expanded_script_task_status".to_owned(),
            latest
                .as_ref()
                .map_or(Value::Null, |task| task["status"].clone()),
        );
        object.insert(
            "expanded_script_error_message".to_owned(),
            latest
                .as_ref()
                .map_or(Value::Null, |task| task["error_message"].clone()),
        );
        object.insert("expanded_script_stage".to_owned(), json!(stage));
        let length = object["expanded_script"]
            .as_str()
            .map(char_count)
            .unwrap_or_default();
        object.insert("expanded_script_length".to_owned(), json!(length));
        Ok(screenplay)
    }

    /// Queue one continuation only after the bootstrap/previous continuation has completed, preserving its story bible checkpoint.
    pub fn continue_screenplay(&self, project_id: &str) -> AppResult<Value> {
        let project = self.repository.raw_drama(project_id)?;
        let latest = self.repository.latest_expansion_task(project_id)?;
        if let Some(task) = latest.as_ref() {
            if task["status"].as_str() == Some(GENERATING) {
                return Ok(task.clone());
            }
            if task["type"].as_str() == Some("script_expansion")
                && matches!(task["status"].as_str(), Some(CANCELLED) | Some(FAILED))
            {
                return self
                    .repository
                    .retry_drama_task(project_id, "script_expansion");
            }
        }
        let expanded = project["expanded_script"]
            .as_str()
            .unwrap_or_default()
            .trim();
        if expanded.is_empty() {
            if latest.as_ref().is_some_and(|task| {
                task["type"].as_str() == Some("script_decomposition")
                    && matches!(task["status"].as_str(), Some(CANCELLED) | Some(FAILED))
            }) {
                return self
                    .repository
                    .restart_drama_task(project_id, "script_decomposition");
            }
            return Err(AppError::BadRequest(
                "请先完成首次剧本扩写后再继续扩写".to_owned(),
            ));
        }
        self.repository.create_active_drama_task(
            project_id,
            "script_expansion",
            None,
            json!({
                "project_id":project_id,
                "continuation_base_length":char_count(expanded),
                "expanded_script_preview":expanded,
                "story_bible":self.repository.latest_expansion_story_bible(project_id)?,
            }),
        )
    }

    /// Cancel only the latest screenplay-owning task and preserve an already failed/cancelled terminal result.
    pub fn cancel_screenplay(&self, project_id: &str) -> AppResult<Value> {
        self.repository.raw_drama(project_id)?;
        let task = self
            .repository
            .latest_expansion_task(project_id)?
            .ok_or_else(|| AppError::BadRequest("未找到可取消的剧本扩写任务".to_owned()))?;
        match task["status"].as_str() {
            Some(FAILED) | Some(CANCELLED) => Ok(task),
            Some(GENERATING) => {
                let cancelled = self
                    .repository
                    .cancel_drama_task(task["id"].as_str().unwrap_or_default(), "剧本扩写已取消")?;
                if task["type"].as_str() == Some("script_decomposition") {
                    self.repository.set_drama_status(project_id, CANCELLED)?;
                }
                Ok(cancelled)
            }
            _ => Err(AppError::BadRequest("剧本扩写已完成，无法取消".to_owned())),
        }
    }
}

fn char_count(value: &str) -> usize {
    value.chars().count()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::{json, Map};

    use super::*;
    use crate::{
        db::Database, media::MediaStore, repository::Repository, value::new_id,
        worker::DurableWorker,
    };

    fn service() -> (DesktopService, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
        let repository = Repository::new(
            Database::open(root.join("ai_application_factory.db")).expect("database"),
        );
        let media = MediaStore::new(repository.clone()).expect("media");
        let worker = DurableWorker::new(repository.clone(), media.clone()).expect("worker");
        (
            DesktopService {
                repository,
                media,
                worker,
            },
            root,
        )
    }

    #[test]
    fn continue_screenplay_restarts_cancelled_empty_bootstrap() {
        let (service, root) = service();
        let project = service
            .repository
            .create_drama(Map::from_iter([
                ("name".to_owned(), json!("从头扩写")),
                (
                    "script".to_owned(),
                    json!("少年在山村长大，得知师门秘密后踏上寻找真相的修行之路。"),
                ),
            ]))
            .expect("project");
        let project_id = project["id"].as_str().expect("project id");
        let cancelled_id = project["task_id"].as_str().expect("task id");
        service
            .repository
            .cancel_drama_task(cancelled_id, "剧本扩写已取消")
            .expect("cancel");
        let restarted = service
            .continue_screenplay(project_id)
            .expect("restart from original");
        assert_ne!(restarted["id"], cancelled_id);
        assert_eq!(restarted["type"], "script_decomposition");
        assert_eq!(restarted["status"], GENERATING);
        fs::remove_dir_all(root).expect("remove test data");
    }

    #[test]
    fn near_limit_continuation_is_allowed_and_can_be_retried() {
        let (service, root) = service();
        let project = service
            .repository
            .create_drama(Map::from_iter([
                ("name".to_owned(), json!("接近上限仍可续写")),
                (
                    "script".to_owned(),
                    json!("主角带着旧信前往车站，在最后一班列车前找到了失踪的朋友。"),
                ),
            ]))
            .expect("project");
        let project_id = project["id"].as_str().expect("project id");
        service
            .repository
            .finish_drama_task(
                project["task_id"].as_str().expect("task id"),
                crate::value::SUCCEEDED,
                None,
                None,
            )
            .expect("finish bootstrap");
        service
            .repository
            .set_expanded_screenplay(project_id, &"剧".repeat(10_000))
            .expect("save near-limit screenplay");

        let continuation = service
            .continue_screenplay(project_id)
            .expect("continue at configured upper bound");
        assert_eq!(continuation["type"], "script_expansion");
        service
            .repository
            .cancel_drama_task(
                continuation["id"].as_str().expect("continuation id"),
                "剧本扩写已取消",
            )
            .expect("cancel continuation");
        let retried = service
            .retry_decomposition(project_id)
            .expect("retry failed continuation from the banner");
        assert_eq!(retried["id"], continuation["id"]);
        assert_eq!(retried["type"], "script_expansion");
        assert_eq!(retried["status"], GENERATING);
        fs::remove_dir_all(root).expect("remove test data");
    }

    #[test]
    fn retry_prefers_a_failed_bootstrap_over_later_successful_continuations() {
        let (service, root) = service();
        let project = service
            .repository
            .create_drama(Map::from_iter([
                ("name".to_owned(), json!("修复首次生成失败")),
                (
                    "script".to_owned(),
                    json!("主角在失火的旧屋里找到录音带，由此发现家族被隐瞒的真相。"),
                ),
            ]))
            .expect("project");
        let project_id = project["id"].as_str().expect("project id");
        let bootstrap_id = project["task_id"].as_str().expect("bootstrap id");
        service
            .repository
            .finish_drama_task(bootstrap_id, FAILED, None, Some("语言模型请求失败"))
            .expect("fail bootstrap");
        let continuation = service
            .repository
            .create_active_drama_task(project_id, "script_expansion", None, json!({}))
            .expect("create continuation");
        service
            .repository
            .finish_drama_task(
                continuation["id"].as_str().expect("continuation id"),
                crate::value::SUCCEEDED,
                None,
                None,
            )
            .expect("finish continuation");

        let retried = service
            .retry_decomposition(project_id)
            .expect("retry failed bootstrap");
        assert_eq!(retried["id"], bootstrap_id);
        assert_eq!(retried["type"], "script_decomposition");
        assert_eq!(retried["status"], GENERATING);
        fs::remove_dir_all(root).expect("remove test data");
    }
}
