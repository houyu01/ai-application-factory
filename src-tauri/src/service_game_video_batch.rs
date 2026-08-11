//! Durable coordinator for toolbar-triggered serial interactive-game node-video generation.

use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    value::{CANCELLED, FAILED, GENERATING, SUCCEEDED},
};

use super::DesktopService;

pub(super) const SERIAL_GAME_VIDEO_BATCH: &str = "serial_game_node_video_batch";
const ALL_NODES: &str = "all";

impl DesktopService {
    /// Start the game-toolbar serial action by persisting one coordinator that queues each node only after the preceding video succeeds.
    pub fn start_serial_game_node_video_batch(&self, game_id: &str) -> AppResult<Value> {
        if let Some(batch) = self
            .repository
            .active_game_tasks(game_id, SERIAL_GAME_VIDEO_BATCH, Some(ALL_NODES))?
            .into_iter()
            .next()
        {
            return Ok(batch);
        }
        let game = self.repository.get_game(game_id)?;
        let nodes = game["nodes"].as_array().cloned().unwrap_or_default();
        if nodes.is_empty() {
            return Err(AppError::BadRequest(
                "当前游戏没有可生成视频的节点".to_owned(),
            ));
        }
        for node in &nodes {
            self.validate_game_node_video_preflight(&game, node)?;
            if !self
                .repository
                .active_game_tasks(game_id, "node_video_generation", node["id"].as_str())?
                .is_empty()
            {
                return Err(AppError::BadRequest(
                    "存在正在生成的节点视频，请完成或取消后再串行生成".to_owned(),
                ));
            }
        }
        let node_ids = nodes
            .iter()
            .filter_map(|node| node["id"].as_str())
            .collect::<Vec<_>>();
        let batch = self.repository.create_active_game_task(
            game_id,
            SERIAL_GAME_VIDEO_BATCH,
            ALL_NODES,
            json!({
                "game_id": game_id,
                "mode": "serial",
                "node_ids": node_ids,
                "total_count": nodes.len(),
                "next_index": 0,
                "completed_count": 0,
                "current_task_id": null,
                "current_node_id": null,
            }),
            "等待串行节点视频生成",
        )?;
        self.advance_serial_game_node_video_batch(
            game_id,
            batch["id"].as_str().unwrap_or_default(),
            None,
        )
    }

    /// Continue a persisted serial run after the WebView extracts the completed node video's tail frame.
    pub fn advance_serial_game_node_video_batch(
        &self,
        game_id: &str,
        batch_id: &str,
        last_frame_data_url: Option<&str>,
    ) -> AppResult<Value> {
        let batch = self.repository.get_game_task(batch_id)?;
        if batch["game_id"].as_str() != Some(game_id) || batch["type"] != SERIAL_GAME_VIDEO_BATCH {
            return Err(AppError::NotFound("串行节点视频批次不存在".to_owned()));
        }
        if batch["status"] != GENERATING {
            return Ok(batch);
        }
        let mut snapshot = batch["input_snapshot"]
            .as_object()
            .cloned()
            .ok_or_else(|| AppError::BadRequest("串行节点视频批次缺少任务数据".to_owned()))?;
        let node_ids = snapshot
            .get("node_ids")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        let current_task_id = snapshot
            .get("current_task_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !current_task_id.is_empty() {
            let child = self.repository.get_game_task(current_task_id)?;
            match child["status"].as_str().unwrap_or_default() {
                GENERATING => return Ok(batch),
                SUCCEEDED => {
                    let completed = snapshot
                        .get("completed_count")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                        + 1;
                    snapshot.insert("completed_count".to_owned(), json!(completed));
                    snapshot.insert("current_task_id".to_owned(), Value::Null);
                    snapshot.insert("current_node_id".to_owned(), Value::Null);
                }
                FAILED => {
                    return self.finish_serial_game_batch(
                        batch_id,
                        FAILED,
                        &snapshot,
                        "上一节点视频生成失败，串行生成已停止",
                    )
                }
                CANCELLED => {
                    return self.finish_serial_game_batch(
                        batch_id,
                        CANCELLED,
                        &snapshot,
                        "上一节点视频已取消，串行生成已停止",
                    )
                }
                _ => return Ok(batch),
            }
        }
        let next = snapshot
            .get("next_index")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        if next >= node_ids.len() {
            return self.finish_serial_game_batch(batch_id, SUCCEEDED, &snapshot, "");
        }
        let first_frame = if next == 0 {
            None
        } else {
            let value = last_frame_data_url
                .filter(|value| value.starts_with("data:image/"))
                .ok_or_else(|| {
                    AppError::BadRequest("请先提取上一节点视频的尾帧，再继续串行生成".to_owned())
                })?;
            Some(self.media.save_data_url(value)?)
        };
        let game = self.repository.get_game(game_id)?;
        let node_id = &node_ids[next];
        let node = self.repository.get_game_node(game_id, node_id)?;
        self.validate_game_node_video_preflight(&game, &node)?;
        if !self
            .repository
            .active_game_tasks(game_id, "node_video_generation", Some(node_id))?
            .is_empty()
        {
            return Err(AppError::BadRequest(
                "下一节点已有正在生成的视频，无法继续串行生成".to_owned(),
            ));
        }
        let task = self.repository.enqueue_game_node_video_with_serial_frame(
            game_id,
            node_id,
            first_frame.as_deref(),
            Some(batch_id),
        )?;
        snapshot.insert("next_index".to_owned(), json!(next + 1));
        snapshot.insert("current_task_id".to_owned(), task["id"].clone());
        snapshot.insert("current_node_id".to_owned(), json!(node_id));
        self.repository
            .update_game_task_snapshot(batch_id, Value::Object(snapshot))?;
        self.repository
            .update_game_task_progress(batch_id, 0, "正在等待当前节点视频")?;
        self.repository.get_game_task(batch_id)
    }

    fn finish_serial_game_batch(
        &self,
        batch_id: &str,
        status: &str,
        snapshot: &Map<String, Value>,
        error: &str,
    ) -> AppResult<Value> {
        let total = snapshot
            .get("total_count")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let completed = snapshot
            .get("completed_count")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        self.repository.finish_game_task(
            batch_id,
            status,
            Some(json!({"total_count": total, "completed_count": completed})),
            (!error.is_empty()).then_some(error),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::{json, Map};

    use crate::{
        db::Database,
        media::MediaStore,
        repository::Repository,
        value::{new_id, SUCCEEDED},
        worker::DurableWorker,
    };

    use super::DesktopService;

    fn service() -> (DesktopService, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("game-serial-video-batch-{}", new_id()));
        let repository = Repository::new(
            Database::open(root.join("ai_application_factory.db")).expect("test database"),
        );
        let media = MediaStore::new(repository.clone()).expect("media store");
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
    fn serial_batch_queues_one_node_and_freezes_the_prior_tail_frame() {
        let (service, root) = service();
        let game = service
            .repository
            .create_game(Map::from_iter([
                ("name".to_owned(), json!("串行节点视频")),
                (
                    "script".to_owned(),
                    json!("玩家在密室中依次破解两处机关，并在每次选择后进入新的视频节点。"),
                ),
            ]))
            .expect("create game");
        let game_id = game["id"].as_str().expect("game id");
        service
            .repository
            .save_game_graph(
                game_id,
                &[],
                &[
                    json!({"id":"first","node_type":"start","title":"入口","original_text":"进入密室","prompt":"玩家推开密室铁门，镜头跟随前进","duration_seconds":10}),
                    json!({"id":"second","node_type":"normal","title":"机关","original_text":"破解机关","prompt":"玩家观察机关并转动转盘","duration_seconds":10}),
                ],
                &[],
            )
            .expect("save graph");
        let nodes = service.repository.get_game(game_id).expect("game")["nodes"]
            .as_array()
            .expect("nodes")
            .to_vec();
        let first_node_id = nodes[0]["id"].as_str().expect("first node id");
        let second_node_id = nodes[1]["id"].as_str().expect("second node id");
        let batch = service
            .start_serial_game_node_video_batch(game_id)
            .expect("start serial batch");
        let batch_id = batch["id"].as_str().expect("batch id");
        let first_id = batch["input_snapshot"]["current_task_id"]
            .as_str()
            .expect("first task id");
        assert_eq!(batch["input_snapshot"]["current_node_id"], first_node_id);
        service
            .repository
            .finish_game_node_video(
                game_id,
                first_node_id,
                first_id,
                Some("/media/first.mp4"),
                SUCCEEDED,
                None,
            )
            .expect("finish first history");
        service
            .repository
            .finish_game_task(first_id, SUCCEEDED, None, None)
            .expect("finish first task");
        let advanced = service
            .advance_serial_game_node_video_batch(
                game_id,
                batch_id,
                Some("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Y9J8e8AAAAASUVORK5CYII="),
            )
            .expect("advance serial batch");
        let second_id = advanced["input_snapshot"]["current_task_id"]
            .as_str()
            .expect("second task id");
        let second = service
            .repository
            .get_game_task(second_id)
            .expect("second task");
        assert_eq!(
            advanced["input_snapshot"]["current_node_id"],
            second_node_id
        );
        assert!(second["input_snapshot"]["serial_first_frame"]
            .as_str()
            .is_some_and(|frame| frame.starts_with("/api/media/")));
        fs::remove_dir_all(root).expect("remove test data");
    }
}
