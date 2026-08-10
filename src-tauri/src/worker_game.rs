//! Interactive-game task execution split from the drama worker to keep durable flows focused.

use serde_json::{json, Value};

use crate::{
    error::AppResult,
    planner,
    value::{now, FAILED, SUCCEEDED},
};

use super::DurableWorker;

impl DurableWorker {
    pub(super) fn run_game(&self, task: Value) {
        let id = task["id"].as_str().unwrap_or_default();
        let game_id = task["game_id"].as_str().unwrap_or_default();
        let result = match task["type"].as_str().unwrap_or_default() {
            "game_graph_decomposition" => {
                let game = self.repository.get_game(game_id);
                game.and_then(|game| {
                    let plan = planner::fallback_game_plan(&game);
                    self.repository.save_game_graph(
                        game_id,
                        plan["assets"].as_array().unwrap_or(&Vec::new()),
                        plan["nodes"].as_array().unwrap_or(&Vec::new()),
                        plan["edges"].as_array().unwrap_or(&Vec::new()),
                    )?;
                    self.repository
                        .finish_game_task(id, SUCCEEDED, Some(plan), None)?;
                    Ok(())
                })
            }
            "node_video_generation" => self.game_video(
                id,
                game_id,
                task["resource_id"].as_str().unwrap_or_default(),
            ),
            other => Err(crate::error::AppError::BadRequest(format!(
                "未知的游戏任务类型：{other}"
            ))),
        };
        if let Err(error) = result {
            let _ = self
                .repository
                .finish_game_task(id, FAILED, None, Some(&error.to_string()));
        }
    }

    fn game_video(&self, id: &str, game_id: &str, node_id: &str) -> AppResult<()> {
        let game = self.repository.get_game(game_id)?;
        game["nodes"]
            .as_array()
            .and_then(|nodes| {
                nodes
                    .iter()
                    .find(|node| node["id"].as_str() == Some(node_id))
            })
            .ok_or_else(|| {
                crate::error::AppError::NotFound(format!("Game node not found: {node_id}"))
            })?;
        self.repository
            .finish_game_node_video(game_id, node_id, id, None, SUCCEEDED, None)?;
        self.repository.finish_game_task(
            id,
            SUCCEEDED,
            Some(json!({"node_id":node_id,"id":id,"url":null,"task_id":id,"generated_at":now()})),
            None,
        )?;
        Ok(())
    }
}
