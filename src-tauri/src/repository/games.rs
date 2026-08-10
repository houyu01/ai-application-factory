//! Interactive-video game graphs, durable tasks, editor mutations, and runtime sessions.

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    mapping,
    value::{
        json_field, json_text, new_id, now, row_to_json, string, GENERATING, NOT_GENERATED,
        SUCCEEDED,
    },
};

use super::{
    game_validation::{game_integer, validate_game_form},
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
        let failure = game_integer(&values, "failure_ending_count", 30, 1, 200)?;
        let branch_min = game_integer(&values, "branch_min", 2, 2, 4)?;
        let branch_max = game_integer(&values, "branch_max", 4, 2, 4)?;
        let duration_min = game_integer(&values, "node_duration_min", 5, 1, 600)?;
        let duration_max = game_integer(&values, "node_duration_max", 30, 1, 600)?;
        let id = new_id();
        let task_id = new_id();
        let timestamp = now();
        self.db.with_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            transaction.execute("INSERT INTO interactive_games (id,name,script,platform,style,success_ending_count,failure_ending_count,branch_min,branch_max,node_duration_min,node_duration_max,language_model,multimodal_model,video_model,status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?16)", params![id,name,script,string(&values,"platform","Steam游戏"),string(&values,"style","真人风格"),success,failure,branch_min,branch_max,duration_min,duration_max,string(&values,"language_model","doubao-seed"),string(&values,"multimodal_model","doubao-seeddream"),string(&values,"video_model","doubao-seedance-2.0"),GENERATING,timestamp])?;
            transaction.execute("INSERT INTO game_tasks (id,game_id,type,status,input_snapshot_json,progress,stage,created_at,started_at) VALUES (?1,?2,'game_graph_decomposition',?3,?4,0,'',?5,?5)", params![task_id,id,GENERATING,json_text(&json!({"game_id":id})),timestamp])?;
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

    /// Create the deterministic fallback graph used when no language-model configuration is available.
    pub fn save_game_graph(
        &self,
        game_id: &str,
        assets: &[Value],
        nodes: &[Value],
        edges: &[Value],
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            let transaction=connection.unchecked_transaction()?; transaction.execute("DELETE FROM game_edges WHERE game_id=?1",[game_id])?; transaction.execute("DELETE FROM game_nodes WHERE game_id=?1",[game_id])?; transaction.execute("DELETE FROM game_assets WHERE game_id=?1",[game_id])?;
            for (index, asset) in assets.iter().enumerate() {
                let source_id = asset["id"].as_str().map(str::to_owned).unwrap_or_else(new_id);
                let id = format!("{game_id}:asset:{source_id}:{index}");
                transaction.execute("INSERT INTO game_assets (id,game_id,type,name,prompt,image_url,status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8)",params![id,game_id,asset["type"].as_str().unwrap_or("prop"),asset["name"].as_str().unwrap_or("素材"),asset["prompt"].as_str().unwrap_or(""),asset["image_url"].as_str(),asset["status"].as_str().unwrap_or(NOT_GENERATED),now()])?;
            }
            let mut identifiers=std::collections::HashMap::new(); for (index,node) in nodes.iter().enumerate(){let original=node["id"].as_str().unwrap_or_default();let id=format!("{game_id}:node:{original}:{index}");identifiers.insert(original.to_owned(),id.clone());transaction.execute("INSERT INTO game_nodes (id,game_id,node_type,title,original_text,prompt,video_url,duration_seconds,status,position_x,position_y,video_history_json,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?13)",params![id,game_id,node["node_type"].as_str().unwrap_or("normal"),node["title"].as_str().unwrap_or("节点"),node["original_text"].as_str().unwrap_or(""),node["prompt"].as_str().unwrap_or(""),node["video_url"].as_str(),node["duration_seconds"].as_i64().unwrap_or(10),node["status"].as_str().unwrap_or(NOT_GENERATED),node["position_x"].as_i64().unwrap_or(80+(index as i64%4)*280),node["position_y"].as_i64().unwrap_or(80+(index as i64/4)*190),json_text(node.get("video_history").unwrap_or(&json!([]))),now()])?;}
            for (index,edge) in edges.iter().enumerate(){let source=identifiers.get(edge["source_node_id"].as_str().unwrap_or_default()).cloned().unwrap_or_else(||edge["source_node_id"].as_str().unwrap_or_default().to_owned());let target=identifiers.get(edge["target_node_id"].as_str().unwrap_or_default()).cloned().unwrap_or_else(||edge["target_node_id"].as_str().unwrap_or_default().to_owned());let source_id=edge["id"].as_str().map(str::to_owned).unwrap_or_else(new_id);transaction.execute("INSERT INTO game_edges (id,game_id,source_node_id,target_node_id,option_text,sort_order,conditions_json,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8)",params![format!("{game_id}:edge:{source_id}:{index}"),game_id,source,target,edge["option_text"].as_str().unwrap_or("继续"),edge["sort_order"].as_i64().unwrap_or(index as i64+1),json_text(edge.get("conditions").unwrap_or(&json!({}))),now()])?;}
            transaction.execute("UPDATE interactive_games SET assets_json=?1,nodes_json=?2,edges_json=?3,status=?4,updated_at=?5 WHERE id=?6",params![json_text(&Value::Array(assets.to_vec())),json_text(&Value::Array(nodes.to_vec())),json_text(&Value::Array(edges.to_vec())),SUCCEEDED,now(),game_id])?;transaction.commit()?;Ok(())
        })
    }

    /// Update fields that the node inspector may change.
    pub fn update_game_node(
        &self,
        game_id: &str,
        node_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            for key in ["title", "original_text", "prompt", "video_url", "status"] {
                if let Some(value) = values.get(key).and_then(Value::as_str) {
                    connection.execute(&format!("UPDATE game_nodes SET {key}=?1,updated_at=?2 WHERE id=?3 AND game_id=?4"), params![value, now(), node_id, game_id])?;
                }
            }
            for key in ["duration_seconds", "position_x", "position_y"] {
                if let Some(value) = values.get(key).and_then(Value::as_i64) {
                    connection.execute(&format!("UPDATE game_nodes SET {key}=?1,updated_at=?2 WHERE id=?3 AND game_id=?4"), params![value, now(), node_id, game_id])?;
                }
            }
            Ok(())
        })?;
        self.get_game_node(game_id, node_id)
    }

    /// Add a choice after checking both source and target belong to the same game.
    pub fn create_game_edge(&self, game_id: &str, values: Map<String, Value>) -> AppResult<Value> {
        let source = string(&values, "source_node_id", "");
        let target = string(&values, "target_node_id", "");
        let option = string(&values, "option_text", "");
        if source.is_empty() || target.is_empty() || option.is_empty() {
            return Err(AppError::BadRequest("请选择节点并填写选项文案".to_owned()));
        }
        self.get_game_node(game_id, &source)?;
        self.get_game_node(game_id, &target)?;
        let id = new_id();
        self.db.with_connection(|connection|{connection.execute("INSERT INTO game_edges (id,game_id,source_node_id,target_node_id,option_text,sort_order,conditions_json,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,'{}',?7,?7)",params![id,game_id,source,target,option,values.get("sort_order").and_then(Value::as_i64).unwrap_or(1),now()])?;Ok(())})?;
        self.get_game_edge(game_id, &id)
    }

    /// Update a choice target, text, or order without allowing it to cross game boundaries.
    pub fn update_game_edge(
        &self,
        game_id: &str,
        edge_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        self.get_game_edge(game_id, edge_id)?;
        if let Some(target) = values.get("target_node_id").and_then(Value::as_str) {
            self.get_game_node(game_id, target)?;
        }
        self.db.with_connection(|connection| {
            for key in ["target_node_id", "option_text"] {
                if let Some(value) = values.get(key).and_then(Value::as_str) {
                    connection.execute(&format!("UPDATE game_edges SET {key}=?1,updated_at=?2 WHERE id=?3 AND game_id=?4"), params![value, now(), edge_id, game_id])?;
                }
            }
            if let Some(value) = values.get("sort_order").and_then(Value::as_i64) {
                connection.execute("UPDATE game_edges SET sort_order=?1,updated_at=?2 WHERE id=?3 AND game_id=?4", params![value, now(), edge_id, game_id])?;
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
        self.db.with_connection(|connection|{let node=connection.query_row("SELECT id FROM game_nodes WHERE game_id=?1 AND node_type='start' ORDER BY created_at LIMIT 1",[game_id],|row|row.get::<_,String>(0)).optional()?.ok_or_else(||AppError::Conflict("游戏图谱还没有准备好".to_owned()))?;let id=new_id();connection.execute("INSERT INTO game_sessions (id,game_id,current_node_id,status,path_json,created_at,updated_at) VALUES (?1,?2,?3,'active','[]',?4,?4)",params![id,game_id,node,now()])?;self.game_session(connection,game_id,&id)})
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
        self.db.with_connection(|connection|{let session=connection.query_row("SELECT * FROM game_sessions WHERE id=?1 AND game_id=?2",params![session_id,game_id],row_to_json).optional()?.ok_or_else(||AppError::NotFound(format!("Game session not found: {session_id}")))?;if session["status"].as_str()!=Some("active"){return Err(AppError::Conflict("Game session has already reached an ending".to_owned()));}let current=session["current_node_id"].as_str().unwrap_or_default();let edge=self.get_game_edge(game_id,edge_id)?;if edge["source_node_id"].as_str()!=Some(current){return Err(AppError::Conflict("The selected edge is not available from the current node".to_owned()));}let target=self.get_game_node(game_id,edge["target_node_id"].as_str().unwrap_or_default())?;let mut path:Vec<Value>=serde_json::from_str(session["path_json"].as_str().unwrap_or("[]")).unwrap_or_default();path.push(json!({"edge_id":edge_id,"source_node_id":current,"target_node_id":target["id"],"option_text":edge["option_text"],"selected_at":now()}));let state=if ["success","failure"].contains(&target["node_type"].as_str().unwrap_or("normal")){"completed"}else{"active"};connection.execute("UPDATE game_sessions SET current_node_id=?1,status=?2,path_json=?3,updated_at=?4 WHERE id=?5",params![target["id"].as_str(),state,json_text(&Value::Array(path)),now(),session_id])?;connection.execute("INSERT INTO game_choice_events (id,session_id,game_id,source_node_id,edge_id,target_node_id,option_text,selected_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![new_id(),session_id,game_id,current,edge_id,target["id"].as_str(),edge["option_text"].as_str(),now()])?;self.game_session(connection,game_id,session_id)})
    }

    /// Create/reuse a durable video task for one game node.
    pub fn enqueue_game_node_video(&self, game_id: &str, node_id: &str) -> AppResult<Value> {
        self.get_game_node(game_id, node_id)?;
        self.db.with_connection(|connection| {
            let existing = connection.query_row("SELECT * FROM game_tasks WHERE game_id=?1 AND type='node_video_generation' AND resource_id=?2 AND status='生成中' ORDER BY created_at DESC LIMIT 1", params![game_id, node_id], row_to_json).optional()?;
            if let Some(task) = existing {
                let mut task = mapping::game_task(task);
                task.as_object_mut().expect("game task is an object").insert("_reused".to_owned(), json!(true));
                return Ok(task);
            }
            let id = new_id();
            connection.execute("INSERT INTO game_tasks (id,game_id,type,resource_id,status,input_snapshot_json,progress,stage,created_at,started_at) VALUES (?1,?2,'node_video_generation',?3,'生成中',?4,0,'',?5,?5)", params![id,game_id,node_id,json_text(&json!({"game_id":game_id,"node_id":node_id})),now()])?;
            connection.execute("UPDATE game_nodes SET status='生成中',updated_at=?1 WHERE id=?2",params![now(),node_id])?;
            let mut task = self.get_game_task(&id)?;
            task.as_object_mut().expect("game task is an object").insert("_reused".to_owned(), json!(false));
            Ok(task)
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
        self.db.with_connection(|connection|{let id=connection.query_row("SELECT id FROM game_tasks WHERE status='生成中' AND (poll_lease_until IS NULL OR poll_lease_until<?1) ORDER BY created_at LIMIT 1",[now()],|row|row.get::<_,String>(0)).optional()?;let Some(id)=id else{return Ok(None);};connection.execute("UPDATE game_tasks SET poll_lease_token=?1,poll_lease_until=?2,poll_attempts=poll_attempts+1 WHERE id=?3",params![new_id(),(chrono::Utc::now()+chrono::Duration::seconds(60)).to_rfc3339(),id])?;self.get_game_task(&id).map(Some)})
    }

    /// Finish a game task and release its worker lease after its graph or node media result is persisted.
    pub fn finish_game_task(
        &self,
        id: &str,
        status: &str,
        result: Option<Value>,
        error: Option<&str>,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection|{if connection.execute("UPDATE game_tasks SET status=?1,result_json=?2,error_message=?3,progress=100,stage='已完成',completed_at=?4,poll_lease_until=NULL,poll_lease_token=NULL WHERE id=?5",params![status,result.as_ref().map(json_text),error,now(),id])?==0{return Err(AppError::NotFound(format!("Game task not found: {id}")));}self.get_game_task(id)})
    }

    /// Append a game node's completed video into its history and expose it as the currently playable URL.
    pub fn finish_game_node_video(
        &self,
        game_id: &str,
        node_id: &str,
        task_id: &str,
        url: Option<&str>,
        status: &str,
        error: Option<&str>,
    ) -> AppResult<()> {
        self.db.with_connection(|connection|{let raw=connection.query_row("SELECT video_history_json FROM game_nodes WHERE id=?1 AND game_id=?2",params![node_id,game_id],|row|row.get::<_,String>(0))?;let mut history:Vec<Value>=serde_json::from_str(&raw).unwrap_or_default();history.push(json!({"id":task_id,"url":url,"generated_at":now(),"task_id":task_id,"status":status,"error_message":error}));connection.execute("UPDATE game_nodes SET video_url=?1,video_history_json=?2,status=?3,updated_at=?4 WHERE id=?5 AND game_id=?6",params![url,json_text(&Value::Array(history)),status,now(),node_id,game_id])?;Ok(())})
    }

    pub(crate) fn game_assets_for(
        &self,
        connection: &rusqlite::Connection,
        id: &str,
    ) -> AppResult<Vec<Value>> {
        let mut statement = connection
            .prepare("SELECT * FROM game_assets WHERE game_id=?1 ORDER BY created_at,id")?;
        let rows = statement
            .query_map([id], row_to_json)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(mapping::game_asset)
            .collect();
        Ok(rows)
    }
    pub(crate) fn game_nodes_for(
        &self,
        connection: &rusqlite::Connection,
        id: &str,
    ) -> AppResult<Vec<Value>> {
        let mut statement = connection
            .prepare("SELECT * FROM game_nodes WHERE game_id=?1 ORDER BY created_at,id")?;
        let rows = statement
            .query_map([id], row_to_json)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(mapping::game_node)
            .collect();
        Ok(rows)
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
    fn get_game_node(&self, game_id: &str, node_id: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT * FROM game_nodes WHERE id=?1 AND game_id=?2",
                    params![node_id, game_id],
                    row_to_json,
                )
                .optional()?
                .map(mapping::game_node)
                .ok_or_else(|| AppError::NotFound(format!("Game node not found: {node_id}")))
        })
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
        object.insert("path".to_owned(), path);
        object.insert("current_node".to_owned(), node);
        object.insert("choices".to_owned(), Value::Array(choices));
        Ok(session)
    }
}
