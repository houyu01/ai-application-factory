//! Persisted short-drama video-export defaults and immutable export input snapshots.

use std::collections::{BTreeMap, HashSet};

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    value::{now, row_to_json, SUCCEEDED},
};

use super::Repository;

impl Repository {
    /// Mark the completed historical video a creator wants the ZIP export dialog to choose by default.
    ///
    /// The video-history card invokes this flow. The repository owns the transaction so every shot has
    /// at most one selected version while retaining every other version for preview, download, or refinement.
    pub fn select_shot_version_for_export(
        &self,
        drama_id: &str,
        shot_id: &str,
        version_id: &str,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            let playable = transaction
                .query_row(
                    "SELECT 1 FROM drama_shot_versions WHERE id=?1 AND drama_id=?2 AND shot_id=?3 AND status=?4 AND COALESCE(video_url,'')<>''",
                    params![version_id, drama_id, shot_id, SUCCEEDED],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !playable {
                return Err(AppError::BadRequest("只能标记已生成完成的视频版本".to_owned()));
            }
            transaction.execute(
                "UPDATE drama_shot_versions SET is_selected_for_export=0 WHERE drama_id=?1 AND shot_id=?2",
                params![drama_id, shot_id],
            )?;
            transaction.execute(
                "UPDATE drama_shot_versions SET is_selected_for_export=1 WHERE id=?1 AND drama_id=?2 AND shot_id=?3",
                params![version_id, drama_id, shot_id],
            )?;
            transaction.execute(
                "UPDATE drama_shots SET updated_at=?1 WHERE id=?2 AND drama_id=?3",
                params![now(), shot_id, drama_id],
            )?;
            transaction.commit()?;
            Ok(())
        })?;
        self.get_shot_version(drama_id, shot_id, version_id)
    }

    /// Resolve and freeze the version selection made in the export dialog before a durable ZIP task exists.
    ///
    /// The worker only reads this snapshot, so later changes to a history-card default cannot silently alter
    /// an already queued export. It skips shots without a selected playable version, letting creators download
    /// every completed portion of a drama before all episodes have finished generating.
    pub fn video_export_snapshot(
        &self,
        drama_id: &str,
        values: &Map<String, Value>,
    ) -> AppResult<Value> {
        let selected = values
            .get("selections")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::BadRequest("请至少选择一个已生成的视频版本".to_owned()))?;
        let mut version_by_shot = BTreeMap::new();
        for item in selected {
            let shot_id = item["shot_id"].as_str().unwrap_or_default().trim();
            let version_id = item["version_id"].as_str().unwrap_or_default().trim();
            if shot_id.is_empty() {
                return Err(AppError::BadRequest("分镜标识不能为空".to_owned()));
            }
            if version_id.is_empty() {
                continue;
            }
            if version_by_shot
                .insert(shot_id.to_owned(), version_id.to_owned())
                .is_some()
            {
                return Err(AppError::BadRequest(
                    "同一分镜只能选择一个视频版本".to_owned(),
                ));
            }
        }
        if version_by_shot.is_empty() {
            return Err(AppError::BadRequest(
                "请至少选择一个已生成的视频版本".to_owned(),
            ));
        }
        self.db.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id,episode_id,episode_name,episode_sort_order,shot_index,title FROM drama_shots WHERE drama_id=?1 ORDER BY episode_sort_order,episode_name,shot_index,created_at,id",
            )?;
            let shots = statement
                .query_map([drama_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            if shots.is_empty() {
                return Err(AppError::BadRequest("当前项目没有可导出的视频分镜".to_owned()));
            }
            let expected = shots.iter().map(|shot| shot.0.as_str()).collect::<HashSet<_>>();
            if version_by_shot.keys().any(|shot| !expected.contains(shot.as_str())) {
                return Err(AppError::BadRequest("选择中包含不属于当前项目的分镜".to_owned()));
            }
            let mut entries = Vec::with_capacity(version_by_shot.len());
            for (shot_id, episode_id, episode_name, episode_order, shot_index, title) in shots {
                let Some(version_id) = version_by_shot.get(&shot_id) else {
                    continue;
                };
                let version = connection
                    .query_row(
                        "SELECT * FROM drama_shot_versions WHERE id=?1 AND drama_id=?2 AND shot_id=?3 AND status=?4 AND COALESCE(video_url,'')<>''",
                        params![version_id, drama_id, shot_id, SUCCEEDED],
                        row_to_json,
                    )
                    .optional()?
                    .ok_or_else(|| AppError::BadRequest(format!("分镜「{title}」选择的视频不可导出")))?;
                entries.push(json!({
                    "shot_id": shot_id,
                    "shot_title": title,
                    "shot_index": shot_index,
                    "episode_id": episode_id,
                    "episode_name": episode_name,
                    "episode_sort_order": episode_order,
                    "version_id": version["id"],
                    "version_no": version["version_no"],
                    "video_url": version["video_url"],
                }));
            }
            Ok(Value::Array(entries))
        })
    }

    /// Read a scoped video-export task so the export dialog can poll its progress and final ZIP URL.
    pub fn video_export_task(&self, drama_id: &str, task_id: &str) -> AppResult<Value> {
        let task = self.get_drama_task(task_id)?;
        if task["project_id"].as_str() != Some(drama_id)
            || task["type"].as_str() != Some("drama_video_export")
        {
            return Err(AppError::NotFound("视频导出任务不存在".to_owned()));
        }
        Ok(task)
    }

    /// Return newest-first cloud archive links retained in successful durable export tasks.
    pub fn video_export_history(&self, drama_id: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT * FROM generation_tasks WHERE drama_id=?1 AND type='drama_video_export' AND status=?2 ORDER BY completed_at DESC,created_at DESC,id DESC",
            )?;
            let tasks = statement
                .query_map(params![drama_id, SUCCEEDED], row_to_json)?
                .collect::<Result<Vec<_>, _>>()?;
            let entries = tasks
                .into_iter()
                .filter(|task| {
                    task["input_snapshot"]["destination"].as_str() == Some("cloud")
                        || (task["input_snapshot"]["destination"].is_null()
                            && task["result"]["url"]
                                .as_str()
                                .is_some_and(|url| url.starts_with("http://") || url.starts_with("https://")))
                })
                .filter_map(|task| {
                    let result = &task["result"];
                    let url = result["url"].as_str()?.to_owned();
                    Some(json!({
                        "task_id": task["id"],
                        "url": url,
                        "file_name": result["file_name"],
                        "created_at": task["created_at"],
                        "completed_at": task["completed_at"],
                    }))
                })
                .collect::<Vec<_>>();
            Ok(Value::Array(entries))
        })
    }
}
