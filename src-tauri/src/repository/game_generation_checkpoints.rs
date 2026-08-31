//! Durable checkpoints for interactive-game screenplay and graph generation.

use rusqlite::params;
use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    value::{json_text, new_id, now, GENERATING},
};

use super::Repository;

impl Repository {
    /// Persist a streamed game screenplay while the creator is watching its expansion task.
    ///
    /// The interactive-game expansion worker calls this at a throttled stream boundary so a restart
    /// can continue from the durable screenplay instead of discarding its generated text.
    pub(crate) fn persist_game_screenplay_preview(
        &self,
        task_id: &str,
        game_id: &str,
        screenplay: &str,
        progress: i64,
        stage: &str,
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            if connection.execute(
                "UPDATE game_tasks SET input_snapshot_json=?1,progress=?2,stage=?3 WHERE id=?4 AND game_id=?5 AND type='game_script_expansion' AND status=?6",
                params![json_text(&json!({"game_id":game_id,"expanded_script_preview":screenplay})), progress.clamp(0, 99), stage, task_id, game_id, GENERATING],
            )? == 0 {
                return Err(AppError::BadRequest("剧本扩写已停止".to_owned()));
            }
            connection.execute(
                "UPDATE interactive_games SET expanded_script=?1,updated_at=?2 WHERE id=?3",
                params![screenplay, now(), game_id],
            )?;
            Ok(())
        })
    }

    /// Atomically finish a screenplay checkpoint and enqueue graph decomposition for a new game.
    ///
    /// The game-creation flow uses this boundary after expansion has become durable, ensuring a
    /// later graph task always starts from saved branch text rather than an in-memory response.
    pub(crate) fn complete_game_screenplay_expansion(
        &self,
        task_id: &str,
        game_id: &str,
        screenplay: &str,
        original_length: usize,
        queue_graph: bool,
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            if transaction.execute(
                "UPDATE game_tasks SET input_snapshot_json=?1,progress=82,stage='扩写剧本已保存，等待拆分视频节点' WHERE id=?2 AND game_id=?3 AND type='game_script_expansion' AND status=?4",
                params![json_text(&json!({"game_id":game_id,"expanded_script_preview":screenplay})), task_id, game_id, GENERATING],
            )? == 0 {
                return Err(AppError::BadRequest("剧本扩写已停止".to_owned()));
            }
            transaction.execute(
                "UPDATE interactive_games SET expanded_script=?1,status=?2,updated_at=?3 WHERE id=?4",
                params![screenplay, if queue_graph { GENERATING } else { crate::value::SUCCEEDED }, now(), game_id],
            )?;
            let graph_task_id = queue_graph.then(new_id);
            if let Some(id) = graph_task_id.as_deref() {
                let timestamp = now();
                transaction.execute(
                    "INSERT INTO game_tasks (id,game_id,type,status,input_snapshot_json,progress,stage,created_at,started_at) VALUES (?1,?2,'game_graph_decomposition',?3,?4,0,'等待图谱生成',?5,?5)",
                    params![id, game_id, GENERATING, json_text(&json!({"game_id":game_id})), timestamp],
                )?;
            }
            transaction.execute(
                "UPDATE game_tasks SET status=?1,result_json=?2,error_message=NULL,progress=100,stage='已完成',completed_at=?3,poll_lease_until=NULL,poll_lease_token=NULL WHERE id=?4 AND status=?5",
                params![crate::value::SUCCEEDED, json_text(&json!({"original_script_length":original_length,"expanded_script_length":screenplay.chars().count(),"next_task_id":graph_task_id})), now(), task_id, GENERATING],
            )?;
            transaction.commit()?;
            Ok(())
        })
    }

    /// Save a validated graph plan before inserting its assets, nodes, and edges.
    ///
    /// The graph worker calls this after the language model has returned a complete valid DAG. If
    /// the desktop app exits while SQLite graph rows are being written, retry reuses this plan and
    /// only completes persistence instead of asking the model to generate the graph again.
    pub(crate) fn persist_game_graph_checkpoint(
        &self,
        task_id: &str,
        game_id: &str,
        plan: &Value,
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            if connection.execute(
                "UPDATE game_tasks SET input_snapshot_json=?1,progress=92,stage='图谱骨架已保存，等待写入视频节点' WHERE id=?2 AND game_id=?3 AND type='game_graph_decomposition' AND status=?4",
                params![json_text(&json!({"game_id":game_id,"graph_checkpoint":plan})), task_id, game_id, GENERATING],
            )? == 0 {
                return Err(AppError::BadRequest("图谱生成已停止".to_owned()));
            }
            Ok(())
        })
    }

    /// Keep a game task's checkpoint visible while retaining its worker lease during a synchronous model call.
    pub(crate) fn update_game_task_progress(
        &self,
        task_id: &str,
        progress: i64,
        stage: &str,
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            if connection.execute(
                "UPDATE game_tasks SET progress=?1,stage=?2 WHERE id=?3 AND status=?4",
                params![progress.clamp(0, 99), stage, task_id, GENERATING],
            )? == 0
            {
                return Err(AppError::BadRequest(format!(
                    "Game task is no longer active: {task_id}"
                )));
            }
            Ok(())
        })
    }

    /// Raise graph-decomposition progress without rewinding a later batch or validation retry.
    pub(crate) fn advance_game_task_progress(
        &self,
        task_id: &str,
        progress: i64,
        stage: &str,
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            if connection.execute(
                "UPDATE game_tasks SET progress=MAX(progress, ?1),stage=?2 WHERE id=?3 AND status=?4",
                params![progress.clamp(0, 99), stage, task_id, GENERATING],
            )? == 0
            {
                return Err(AppError::BadRequest(format!(
                    "Game task is no longer active: {task_id}"
                )));
            }
            Ok(())
        })
    }

    /// Atomically persist a graph stream preview, its accepted records, and visible progress.
    ///
    /// The graph worker calls this at its throttled stream boundary so one checkpoint produces one
    /// SQLite write instead of separate snapshot and progress updates. Progress only moves forward
    /// so a later node batch or edge retry cannot rewind the workbench meter.
    pub(crate) fn persist_game_graph_preview_state(
        &self,
        task_id: &str,
        snapshot: &Value,
        progress: i64,
        stage: &str,
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            if connection.execute(
                "UPDATE game_tasks SET input_snapshot_json=?1,progress=MAX(progress, ?2),stage=?3 WHERE id=?4 AND type='game_graph_decomposition' AND status=?5",
                params![json_text(snapshot), progress.clamp(0, 99), stage, task_id, GENERATING],
            )? == 0
            {
                return Err(AppError::BadRequest(format!(
                    "Game graph task is no longer active: {task_id}"
                )));
            }
            Ok(())
        })
    }

    /// Persist a creator-visible game-generation preview while the model stream is incomplete.
    pub(crate) fn update_game_task_snapshot(
        &self,
        task_id: &str,
        snapshot: Value,
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            if connection.execute(
                "UPDATE game_tasks SET input_snapshot_json=?1 WHERE id=?2 AND status=?3",
                params![json_text(&snapshot), task_id, GENERATING],
            )? == 0
            {
                return Err(AppError::BadRequest(format!(
                    "Game task is no longer active: {task_id}"
                )));
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::{json, Map};

    use crate::{
        db::Database,
        media::MediaStore,
        planner,
        repository::Repository,
        value::{new_id, FAILED, GENERATING, SUCCEEDED},
        worker::DurableWorker,
    };

    #[test]
    fn graph_checkpoint_survives_an_app_restart_and_retry() {
        let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
        let path = root.join("ai_application_factory.db");
        let repository = Repository::new(Database::open(path.clone()).expect("test database"));
        let game = repository
            .create_game(Map::from_iter([
                ("name".to_owned(), json!("断点图谱")),
                (
                    "script".to_owned(),
                    json!("玩家在钟楼收到失踪同伴的录音，必须在警报响起前找出真相。"),
                ),
                ("success_ending_count".to_owned(), json!(1)),
                ("failure_ending_count".to_owned(), json!(1)),
            ]))
            .expect("create game");
        let game_id = game["id"].as_str().expect("game id");
        let expansion_task = game["task"]["id"].as_str().expect("expansion task");
        repository
            .complete_game_screenplay_expansion(
                expansion_task,
                game_id,
                "【剧情段 S01｜开始】\n【玩家抉择】\n【结局 E01｜成功】\n【结局 E02｜失败】",
                20,
                true,
            )
            .expect("queue graph task");
        let graph_task = repository.get_game(game_id).expect("load graph task")["tasks"]
            .as_array()
            .expect("tasks")
            .iter()
            .find(|task| task["type"] == "game_graph_decomposition")
            .cloned()
            .expect("graph task");
        let graph_task_id = graph_task["id"].as_str().expect("graph task id");
        let plan = planner::fallback_game_plan(&repository.get_game(game_id).expect("game"));
        repository
            .persist_game_graph_checkpoint(graph_task_id, game_id, &plan)
            .expect("save graph checkpoint");

        Database::open(path).expect("restart database");
        assert_eq!(
            repository
                .get_game_task(graph_task_id)
                .expect("failed task")["status"],
            FAILED
        );
        let retried = repository
            .retry_game_generation(game_id)
            .expect("retry graph task");

        assert_eq!(retried["status"], GENERATING);
        assert_eq!(retried["input_snapshot"]["graph_checkpoint"], plan);
        let worker = DurableWorker::new(
            repository.clone(),
            MediaStore::new(repository.clone()).expect("media store"),
        )
        .expect("worker");
        assert!(worker.process_once().expect("resume graph persistence"));
        let resumed = repository.get_game(game_id).expect("resumed graph");
        assert!(resumed["nodes"]
            .as_array()
            .is_some_and(|nodes| !nodes.is_empty()));
        assert_eq!(
            repository
                .get_game_task(graph_task_id)
                .expect("finished task")["status"],
            SUCCEEDED
        );
        fs::remove_dir_all(root).expect("remove test data");
    }

    #[test]
    fn graph_record_checkpoint_survives_a_failed_retry() {
        let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
        let repository = Repository::new(
            Database::open(root.join("ai_application_factory.db")).expect("test database"),
        );
        let game = repository
            .create_game(Map::from_iter([
                ("name".to_owned(), json!("节点断点")),
                (
                    "script".to_owned(),
                    json!("玩家在钟楼收到失踪同伴的录音，必须在警报响起前找出真相。"),
                ),
                ("success_ending_count".to_owned(), json!(1)),
                ("failure_ending_count".to_owned(), json!(1)),
            ]))
            .expect("create game");
        let game_id = game["id"].as_str().expect("game id");
        repository
            .complete_game_screenplay_expansion(
                game["task"]["id"].as_str().expect("expansion task"),
                game_id,
                "【剧情段 S01｜开始】\n【玩家抉择】\n【结局 E01｜成功】\n【结局 E02｜失败】",
                20,
                true,
            )
            .expect("queue graph task");
        let graph_task = repository.get_game(game_id).expect("load graph task")["tasks"]
            .as_array()
            .expect("tasks")
            .iter()
            .find(|task| task["type"] == "game_graph_decomposition")
            .cloned()
            .expect("graph task");
        let graph_task_id = graph_task["id"].as_str().expect("graph task id");
        let checkpoint = planner::game_graph_progress_checkpoint(
            &json!({
                "assets": [
                    {"id":"detective","type":"character","name":"调查员","prompt":"钟楼调查员"}
                ],
                "nodes": [
                    {"id":"start","node_type":"start","title":"入口","original_text":"调查员进入钟楼。","prompt":"钟楼入口。"},
                    {"id":"bad","node_type":"invalid","title":"坏节点","original_text":"格式错误节点。","prompt":"不会保存。"}
                ],
                "edges": [
                    {"id":"branch","source_node_id":"start","target_node_id":"bad","option_text":"沿着钟声前进"}
                ]
            })
            .to_string(),
            None,
        );
        repository
            .update_game_task_snapshot(
                graph_task_id,
                json!({
                    "game_id":game_id,
                    "graph_progress_checkpoint":checkpoint,
                    "graph_generation_stage":"nodes"
                }),
            )
            .expect("save record checkpoint");
        repository
            .finish_game_task(graph_task_id, FAILED, None, Some("节点格式错误"))
            .expect("fail graph task");

        let retried = repository
            .retry_game_generation(game_id)
            .expect("retry graph task");

        assert_eq!(
            retried["input_snapshot"]["graph_progress_checkpoint"]["nodes"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            retried["input_snapshot"]["graph_progress_checkpoint"]["nodes"][0]["id"],
            "start"
        );
        assert_eq!(
            retried["input_snapshot"]["graph_progress_checkpoint"]["edges"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            planner::game_graph_stage(
                &retried["input_snapshot"]["graph_progress_checkpoint"],
                &repository.get_game(game_id).expect("reload game"),
            ),
            planner::GameGraphStage::Nodes
        );
        fs::remove_dir_all(root).expect("remove test data");
    }
}
