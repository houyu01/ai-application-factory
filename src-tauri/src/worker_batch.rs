//! Bounded five-at-a-time image batch coordinator with restart-safe child task tracking.

use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    value::{CANCELLED, FAILED, GENERATING, SUCCEEDED},
};

use super::DurableWorker;

impl DurableWorker {
    pub(super) fn asset_batch(&self, id: &str, project_id: &str, task: &Value) -> AppResult<()> {
        let mut snapshot = task["input_snapshot"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        let jobs = snapshot
            .get("jobs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if jobs.is_empty() {
            return Err(AppError::BadRequest(
                "素材批次没有可执行的图片任务".to_owned(),
            ));
        }
        let active = snapshot
            .get("active_task_ids")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if !active.is_empty() {
            let states = active
                .iter()
                .filter_map(|child| child.as_str())
                .map(|child| self.repository.get_drama_task(child))
                .collect::<Result<Vec<_>, _>>()?;
            if states.iter().any(|child| child["status"] == GENERATING) {
                return self.defer_batch(id, &snapshot, jobs.len());
            }
            let failed = states
                .iter()
                .filter(|child| child["status"] == FAILED)
                .count();
            let cancelled = states
                .iter()
                .filter(|child| child["status"] == CANCELLED)
                .count();
            increment(&mut snapshot, "completed_count", active.len() as i64);
            increment(&mut snapshot, "failed_count", failed as i64);
            increment(&mut snapshot, "cancelled_count", cancelled as i64);
            snapshot.insert("active_task_ids".to_owned(), json!([]));
        }
        let next = snapshot
            .get("next_index")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        if next >= jobs.len() {
            self.repository
                .update_drama_task_snapshot(id, Value::Object(snapshot.clone()))?;
            return self.repository.finish_drama_task(id, SUCCEEDED, Some(json!({"total_count":jobs.len(),"completed_count":snapshot["completed_count"],"failed_count":snapshot["failed_count"],"cancelled_count":snapshot["cancelled_count"]})), None).map(|_| ());
        }
        let end = (next
            + snapshot
                .get("batch_size")
                .and_then(Value::as_u64)
                .unwrap_or(5)
                .clamp(1, 5) as usize)
            .min(jobs.len());
        let mut children = Vec::new();
        for job in &jobs[next..end] {
            children.push(self.enqueue_batch_child(project_id, job, id)?);
        }
        snapshot.insert("next_index".to_owned(), json!(end));
        snapshot.insert("active_task_ids".to_owned(), json!(children));
        self.repository
            .update_drama_task_snapshot(id, Value::Object(snapshot.clone()))?;
        self.defer_batch(id, &snapshot, jobs.len())
    }

    fn enqueue_batch_child(
        &self,
        project_id: &str,
        job: &Value,
        parent: &str,
    ) -> AppResult<String> {
        let asset = job["asset_id"]
            .as_str()
            .ok_or_else(|| AppError::BadRequest("素材批次任务缺少 asset_id".to_owned()))?;
        let kind = job["type"].as_str().unwrap_or("asset_image");
        let (resource, snapshot) = if kind == "asset_variant_image" {
            let variant = job["variant_id"]
                .as_str()
                .ok_or_else(|| AppError::BadRequest("素材形态任务缺少 variant_id".to_owned()))?;
            self.repository
                .set_asset_variant_status(project_id, asset, variant, GENERATING)?;
            (
                variant,
                json!({"project_id":project_id,"asset_id":asset,"variant_id":variant,"parent_task_id":parent}),
            )
        } else {
            self.repository
                .set_asset_status(project_id, asset, GENERATING)?;
            (
                asset,
                json!({"project_id":project_id,"asset_id":asset,"parent_task_id":parent}),
            )
        };
        Ok(self
            .repository
            .create_active_drama_task(project_id, kind, Some(resource), snapshot)?["id"]
            .as_str()
            .unwrap_or_default()
            .to_owned())
    }

    fn defer_batch(
        &self,
        id: &str,
        snapshot: &serde_json::Map<String, Value>,
        total: usize,
    ) -> AppResult<()> {
        let complete = snapshot
            .get("completed_count")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let end = snapshot
            .get("next_index")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .min(total as i64);
        self.repository.reschedule_drama_task(
            id,
            1,
            &format!("正在生成第 {}-{} / {} 张素材图片", complete + 1, end, total),
            None,
        )
    }
}

fn increment(snapshot: &mut serde_json::Map<String, Value>, key: &str, value: i64) {
    let existing = snapshot.get(key).and_then(Value::as_i64).unwrap_or(0);
    snapshot.insert(key.to_owned(), json!(existing + value));
}
