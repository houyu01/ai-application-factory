//! Interactive-video game creation, aggregate retrieval, durable tasks, and runtime sessions.

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    mapping,
    value::{json_field, json_text, new_id, now, row_to_json, string, GENERATING},
};

use super::{
    game_graph_validation::ensure_acyclic_edge,
    game_state::{apply_state_changes, conditions_match, normalize_edge_conditions, session_state},
    game_validation::{game_integer, validate_game_form, GAME_VIDEO_DURATION_RANGE},
    Repository,
};

impl Repository {
    /// Persist an interactive-game shell and graph-planning task before the worker starts.
    pub fn create_game(&self, values: Map<String, Value>) -> AppResult<Value> {
        let name = string(&values, "name", "");
        let script = string(&values, "script", "");
        validate_game_form(&values)?;
        if script.chars().count() < 20 {
            return Err(AppError::BadRequest("剧本文本不少于 20 个字".to_owned()));
        }
        let success = game_integer(&values, "success_ending_count", 2, 1, 100)?;
        let failure = game_integer(&values, "failure_ending_count", 12, 1, 200)?;
        let branch_min = game_integer(&values, "branch_min", 2, 2, 4)?;
        let branch_max = game_integer(&values, "branch_max", 4, 2, 4)?;
        let duration_min = game_integer(
            &values,
            "node_duration_min",
            5,
            *GAME_VIDEO_DURATION_RANGE.start(),
            *GAME_VIDEO_DURATION_RANGE.end(),
        )?;
        let duration_max = game_integer(
            &values,
            "node_duration_max",
            15,
            *GAME_VIDEO_DURATION_RANGE.start(),
            *GAME_VIDEO_DURATION_RANGE.end(),
        )?;
        let expansion_min =
            game_integer(&values, "expanded_script_min_chars", 5_000, 1, 1_000_000)?;
        let expansion_max =
            game_integer(&values, "expanded_script_max_chars", 10_000, 1, 1_000_000)?;
        let node_script_max = game_integer(&values, "node_script_max_chars", 400, 1, 1_000_000)?;
        let id = new_id();
        let task_id = new_id();
        let timestamp = now();
        self.db.with_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            transaction.execute("INSERT INTO interactive_games (id,name,script,platform,style,success_ending_count,failure_ending_count,branch_min,branch_max,node_duration_min,node_duration_max,language_model,multimodal_model,video_model,resolution,enable_web_search,expanded_script_min_chars,expanded_script_max_chars,node_script_max_chars,status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?21)", params![id,name,script,string(&values,"platform","Steam游戏"),string(&values,"style","真人风格"),success,failure,branch_min,branch_max,duration_min,duration_max,string(&values,"language_model","doubao-seed"),string(&values,"multimodal_model","doubao-seeddream"),string(&values,"video_model","doubao-seedance-2.0"),string(&values,"resolution","720p"),values.get("enable_web_search").and_then(Value::as_bool).unwrap_or(false),expansion_min,expansion_max,node_script_max,GENERATING,timestamp])?;
            transaction.execute("INSERT INTO game_tasks (id,game_id,type,status,input_snapshot_json,progress,stage,created_at,started_at) VALUES (?1,?2,'game_script_expansion',?3,?4,0,'等待扩写剧本',?5,?5)", params![task_id,id,GENERATING,json_text(&json!({"game_id":id})),timestamp])?;
            transaction.commit()?; Ok(())
        })?;
        let mut game = self.get_game(&id)?;
        game.as_object_mut()
            .expect("game object")
            .insert("task".to_owned(), self.get_game_task(&task_id)?);
        Ok(game)
    }

    /// Return every game aggregate with nodes, edges, assets, and in-flight task cards.
    pub fn list_games(&self) -> AppResult<Vec<Value>> {
        self.db.with_connection(|connection| {
            let mut statement =
                connection.prepare("SELECT id FROM interactive_games ORDER BY created_at DESC")?;
            let ids = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            ids.into_iter().map(|id| self.get_game(&id)).collect()
        })
    }

    /// Return one editor graph scoped to the owning game.
    pub fn get_game(&self, id: &str) -> AppResult<Value> {
        self.fail_expired_game_generation_tasks()?;
        self.db.with_connection(|connection| {
            let mut game = connection
                .query_row(
                    "SELECT * FROM interactive_games WHERE id=?1",
                    [id],
                    row_to_json,
                )
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("Interactive game not found: {id}")))?;
            let assets = self.game_assets_for(connection, id)?;
            let nodes = self.game_nodes_for(connection, id)?;
            let edges = self.game_edges_for(connection, id)?;
            let tasks = self.game_tasks_for(connection, id)?;
            let object = game.as_object_mut().expect("game object");
            let enable_web_search = object
                .get("enable_web_search")
                .and_then(|value| {
                    value
                        .as_bool()
                        .or_else(|| value.as_i64().map(|value| value != 0))
                })
                .unwrap_or(false);
            object.insert("enable_web_search".to_owned(), json!(enable_web_search));
            let public_prompts = json_field(object, "asset_public_prompts_json", json!({}));
            object.insert("asset_public_prompts".to_owned(), public_prompts);
            object.insert("assets".to_owned(), Value::Array(assets));
            object.insert("nodes".to_owned(), Value::Array(nodes));
            object.insert("edges".to_owned(), Value::Array(edges));
            object.insert("tasks".to_owned(), Value::Array(tasks));
            Ok(game)
        })
    }

    /// Delete a complete game, its playable sessions, and dependent database rows using foreign-key cascades.
    pub fn delete_game(&self, id: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            if connection.execute("DELETE FROM interactive_games WHERE id=?1", [id])? == 0 {
                return Err(AppError::NotFound(format!(
                    "Interactive game not found: {id}"
                )));
            }
            Ok(json!({"status":"deleted","id":id}))
        })
    }

    /// Save model selections shown in the game editor's global parameters modal.
    pub fn update_game_models(&self, id: &str, values: Map<String, Value>) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            for key in ["language_model", "multimodal_model", "video_model"] {
                if let Some(value) = values.get(key).and_then(Value::as_str) {
                    if value.trim().is_empty() {
                        return Err(AppError::BadRequest(format!("{key} 不能为空")));
                    }
                    connection.execute(
                        &format!("UPDATE interactive_games SET {key}=?1,updated_at=?2 WHERE id=?3"),
                        params![value, now(), id],
                    )?;
                }
            }
            Ok(())
        })?;
        self.get_game(id)
    }

    /// Add one creator-defined choice after verifying its endpoints keep the game graph acyclic.
    pub fn create_game_edge(&self, game_id: &str, values: Map<String, Value>) -> AppResult<Value> {
        let source = string(&values, "source_node_id", "");
        let target = string(&values, "target_node_id", "");
        let option = string(&values, "option_text", "");
        let conditions = normalize_edge_conditions(values.get("conditions"))?;
        if source.is_empty() || target.is_empty() || option.is_empty() {
            return Err(AppError::BadRequest("请选择节点并填写选项文案".to_owned()));
        }
        self.get_game_node(game_id, &source)?;
        self.get_game_node(game_id, &target)?;
        let id = new_id();
        self.db.with_connection(|connection| {
            ensure_acyclic_edge(connection, game_id, None, &source, &target)?;
            connection.execute(
                "INSERT INTO game_edges (id,game_id,source_node_id,target_node_id,option_text,sort_order,conditions_json,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8)",
                params![id,game_id,source,target,option,values.get("sort_order").and_then(Value::as_i64).unwrap_or(1),json_text(&conditions),now()],
            )?;
            Ok(())
        })?;
        self.get_game_edge(game_id, &id)
    }

    /// Update a choice source, target, text, or order while preserving the game DAG.
    pub fn update_game_edge(
        &self,
        game_id: &str,
        edge_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        let current = self.get_game_edge(game_id, edge_id)?;
        let conditions = values
            .get("conditions")
            .map(|value| normalize_edge_conditions(Some(value)))
            .transpose()?;
        let source = values
            .get("source_node_id")
            .and_then(Value::as_str)
            .unwrap_or_else(|| current["source_node_id"].as_str().unwrap_or_default());
        let target = values
            .get("target_node_id")
            .and_then(Value::as_str)
            .unwrap_or_else(|| current["target_node_id"].as_str().unwrap_or_default());
        let changes_route =
            values.contains_key("source_node_id") || values.contains_key("target_node_id");
        if changes_route {
            self.get_game_node(game_id, source)?;
            self.get_game_node(game_id, target)?;
        }
        self.db.with_connection(|connection| {
            if changes_route {
                ensure_acyclic_edge(connection, game_id, Some(edge_id), source, target)?;
            }
            for key in ["source_node_id", "target_node_id", "option_text"] {
                if let Some(value) = values.get(key).and_then(Value::as_str) {
                    connection.execute(&format!("UPDATE game_edges SET {key}=?1,updated_at=?2 WHERE id=?3 AND game_id=?4"), params![value, now(), edge_id, game_id])?;
                }
            }
            if let Some(value) = values.get("sort_order").and_then(Value::as_i64) {
                connection.execute("UPDATE game_edges SET sort_order=?1,updated_at=?2 WHERE id=?3 AND game_id=?4", params![value, now(), edge_id, game_id])?;
            }
            if let Some(conditions) = conditions.as_ref() {
                connection.execute("UPDATE game_edges SET conditions_json=?1,updated_at=?2 WHERE id=?3 AND game_id=?4", params![json_text(conditions), now(), edge_id, game_id])?;
            }
            Ok(())
        })?;
        self.get_game_edge(game_id, edge_id)
    }

    /// Delete one selectable edge selected in the graph inspector.
    pub fn delete_game_edge(&self, game_id: &str, edge_id: &str) -> AppResult<()> {
        self.db.with_connection(|connection| {
            if connection.execute(
                "DELETE FROM game_edges WHERE id=?1 AND game_id=?2",
                params![edge_id, game_id],
            )? == 0
            {
                return Err(AppError::NotFound(format!(
                    "Game edge not found: {edge_id}"
                )));
            }
            Ok(())
        })
    }

    /// Begin a runtime session at the first start node and return its playable current-node projection.
    pub fn create_game_session(&self, game_id: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let node = connection
                .query_row(
                    "SELECT id FROM game_nodes WHERE game_id=?1 AND node_type='start' ORDER BY created_at LIMIT 1",
                    [game_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| AppError::Conflict("游戏图谱还没有准备好".to_owned()))?;
            let id = new_id();
            connection.execute(
                "INSERT INTO game_sessions (id,game_id,current_node_id,status,path_json,state_json,created_at,updated_at) VALUES (?1,?2,?3,'active','[]','{}',?4,?4)",
                params![id,game_id,node,now()],
            )?;
            self.game_session(connection, game_id, &id)
        })
    }

    /// Load one runtime session only when it belongs to the route's game id.
    pub fn get_game_session(&self, game_id: &str, session_id: &str) -> AppResult<Value> {
        self.db
            .with_connection(|connection| self.game_session(connection, game_id, session_id))
    }

    /// Record a choice, advance the session, and mark it complete for success/failure endings.
    pub fn choose_game_edge(
        &self,
        game_id: &str,
        session_id: &str,
        edge_id: &str,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let session = connection
                .query_row(
                    "SELECT * FROM game_sessions WHERE id=?1 AND game_id=?2",
                    params![session_id, game_id],
                    row_to_json,
                )
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("Game session not found: {session_id}")))?;
            if session["status"].as_str() != Some("active") {
                return Err(AppError::Conflict(
                    "Game session has already reached an ending".to_owned(),
                ));
            }
            let current = session["current_node_id"].as_str().unwrap_or_default();
            let edge = self.get_game_edge(game_id, edge_id)?;
            if edge["source_node_id"].as_str() != Some(current) {
                return Err(AppError::Conflict(
                    "The selected edge is not available from the current node".to_owned(),
                ));
            }
            let mut path: Vec<Value> = serde_json::from_str(session["path_json"].as_str().unwrap_or("[]")).unwrap_or_default();
            let mut choice_state = session_state(&session);
            if !conditions_match(&choice_state, &edge["conditions"]) {
                return Err(AppError::Conflict("此前选择导致此选项不可用".to_owned()));
            }
            apply_state_changes(&mut choice_state, &edge["conditions"]);
            let target = self.get_game_node(game_id, edge["target_node_id"].as_str().unwrap_or_default())?;
            path.push(json!({"edge_id":edge_id,"source_node_id":current,"target_node_id":target["id"],"option_text":edge["option_text"],"state_changes":edge["conditions"]["set"],"selected_at":now()}));
            let status = if ["success", "failure"].contains(&target["node_type"].as_str().unwrap_or("normal")) { "completed" } else { "active" };
            connection.execute("UPDATE game_sessions SET current_node_id=?1,status=?2,path_json=?3,state_json=?4,updated_at=?5 WHERE id=?6", params![target["id"].as_str(),status,json_text(&Value::Array(path)),json_text(&choice_state),now(),session_id])?;
            connection.execute("INSERT INTO game_choice_events (id,session_id,game_id,source_node_id,edge_id,target_node_id,option_text,selected_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![new_id(),session_id,game_id,current,edge_id,target["id"].as_str(),edge["option_text"].as_str(),now()])?;
            self.game_session(connection, game_id, session_id)
        })
    }

    /// Read a game task for the node-video polling request.
    pub fn get_game_task(&self, id: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            connection
                .query_row("SELECT * FROM game_tasks WHERE id=?1", [id], row_to_json)
                .optional()?
                .map(mapping::game_task)
                .ok_or_else(|| AppError::NotFound(format!("Game task not found: {id}")))
        })
    }

    /// Claim a game task with the same durable SQLite lease behavior used for drama tasks.
    pub fn claim_game_task(&self) -> AppResult<Option<Value>> {
        self.claim_game_task_types(&[
            "game_script_expansion",
            "game_graph_decomposition",
            "game_node_prompt",
            "node_video_generation",
            "game_asset_image",
            "game_asset_variant_image",
            "game_cover_image",
            "game_placeholder_image",
        ])
    }

    /// Finish a game task and release its worker lease after its graph or node media result is persisted.
    pub fn finish_game_task(
        &self,
        id: &str,
        status: &str,
        result: Option<Value>,
        error: Option<&str>,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection|{if connection.execute("UPDATE game_tasks SET status=?1,result_json=?2,error_message=?3,progress=100,stage='已完成',completed_at=?4,poll_lease_until=NULL,poll_lease_token=NULL WHERE id=?5 AND status=?6",params![status,result.as_ref().map(json_text),error,now(),id,GENERATING])?==0{return self.get_game_task(id);}self.get_game_task(id)})
    }

    pub(crate) fn game_edges_for(
        &self,
        connection: &rusqlite::Connection,
        id: &str,
    ) -> AppResult<Vec<Value>> {
        let mut statement = connection
            .prepare("SELECT * FROM game_edges WHERE game_id=?1 ORDER BY sort_order,created_at")?;
        let rows = statement
            .query_map([id], row_to_json)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(mapping::game_edge)
            .collect();
        Ok(rows)
    }
    pub(crate) fn game_tasks_for(
        &self,
        connection: &rusqlite::Connection,
        id: &str,
    ) -> AppResult<Vec<Value>> {
        let mut statement =
            connection.prepare("SELECT * FROM game_tasks WHERE game_id=?1 ORDER BY created_at")?;
        let rows = statement
            .query_map([id], row_to_json)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(mapping::game_task)
            .collect();
        Ok(rows)
    }
    fn get_game_edge(&self, game_id: &str, edge_id: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT * FROM game_edges WHERE id=?1 AND game_id=?2",
                    params![edge_id, game_id],
                    row_to_json,
                )
                .optional()?
                .map(mapping::game_edge)
                .ok_or_else(|| AppError::NotFound(format!("Game edge not found: {edge_id}")))
        })
    }
    fn game_session(
        &self,
        connection: &rusqlite::Connection,
        game_id: &str,
        session_id: &str,
    ) -> AppResult<Value> {
        let mut session = connection
            .query_row(
                "SELECT * FROM game_sessions WHERE id=?1 AND game_id=?2",
                params![session_id, game_id],
                row_to_json,
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("Game session not found: {session_id}")))?;
        let current = session["current_node_id"].as_str().unwrap_or_default();
        let node = self.get_game_node(game_id, current)?;
        let mut statement=connection.prepare("SELECT * FROM game_edges WHERE game_id=?1 AND source_node_id=?2 ORDER BY sort_order,created_at")?;
        let choices = statement
            .query_map(params![game_id, current], row_to_json)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(mapping::game_edge)
            .collect::<Vec<_>>();
        let object = session.as_object_mut().expect("session");
        let path = json_field(object, "path_json", json!([]));
        let state = json_field(object, "state_json", json!({}));
        object.insert("path".to_owned(), path);
        object.insert("state".to_owned(), state.clone());
        object.insert("current_node".to_owned(), node);
        object.insert(
            "choices".to_owned(),
            Value::Array(
                choices
                    .into_iter()
                    .filter(|edge| conditions_match(&state, &edge["conditions"]))
                    .collect(),
            ),
        );
        Ok(session)
    }
}
