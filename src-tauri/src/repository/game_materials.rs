//! Interactive-game material and node-video persistence owned entirely by SQLite.

use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    mapping,
    repository::game_validation::GAME_VIDEO_DURATION_RANGE,
    value::{json_text, new_id, now, row_to_json, NOT_GENERATED, SUCCEEDED},
};

use super::Repository;

impl Repository {
    /// Persist the graph planner's base assets and nodes, adding editable placeholder and cover materials ready for later image generation.
    pub fn save_game_graph(
        &self,
        game_id: &str,
        assets: &[Value],
        nodes: &[Value],
        edges: &[Value],
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            transaction.execute("DELETE FROM game_edges WHERE game_id=?1", [game_id])?;
            transaction.execute("DELETE FROM game_nodes WHERE game_id=?1", [game_id])?;
            transaction.execute("DELETE FROM game_assets WHERE game_id=?1", [game_id])?;
            let materials = complete_materials(assets);
            let mut asset_ids = HashMap::new();
            let mut saved_assets = Vec::new();
            for (index, asset) in materials.iter().enumerate() {
                let source_id = asset["id"].as_str().map(str::to_owned).unwrap_or_else(new_id);
                let id = format!("{game_id}:asset:{source_id}:{index}");
                let name = asset["name"].as_str().unwrap_or("素材");
                asset_ids.insert(source_id, id.clone());
                asset_ids.entry(name.to_owned()).or_insert_with(|| id.clone());
                let kind = asset["type"].as_str().unwrap_or("prop");
                let voice_id = if kind == "character" {
                    Self::normalise_voice_id(&transaction, asset.get("voice_id"))?
                } else { None };
                transaction.execute(
                    "INSERT INTO game_assets (id,game_id,type,name,prompt,voice_id,image_url,status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",
                    params![id,game_id,kind,name,asset["prompt"].as_str().unwrap_or(""),voice_id,asset["image_url"].as_str(),asset["status"].as_str().unwrap_or(NOT_GENERATED),now()],
                )?;
                saved_assets.push(json!({"id":id,"type":kind,"name":name,"prompt":asset["prompt"],"voice_id":voice_id,"image_url":asset["image_url"],"status":asset["status"].as_str().unwrap_or(NOT_GENERATED)}));
            }
            let mut node_ids = HashMap::new();
            let mut saved_nodes = Vec::new();
            for (index, node) in nodes.iter().enumerate() {
                let source_id = node["id"].as_str().unwrap_or_default();
                let id = format!("{game_id}:node:{source_id}:{index}");
                node_ids.insert(source_id.to_owned(), id.clone());
                let references = mapped_references(node, &asset_ids);
                let frames = mapped_frames(node, &asset_ids);
                let placeholder = mapped_asset_id(node["placeholder_asset_id"].as_str(), &asset_ids);
                let placeholder_scene = mapped_asset_id(node["placeholder_scene_asset_id"].as_str(), &asset_ids);
                let placeholder_placements = node.get("placeholder_placements").cloned().unwrap_or_else(|| json!([]));
                let history = node.get("video_history").cloned().unwrap_or_else(|| json!([]));
                let duration = node["duration_seconds"]
                    .as_i64()
                    .unwrap_or(10)
                    .clamp(
                        *GAME_VIDEO_DURATION_RANGE.start(),
                        *GAME_VIDEO_DURATION_RANGE.end(),
                    );
                transaction.execute(
                    "INSERT INTO game_nodes (id,game_id,node_type,title,original_text,prompt,prompt_rich_json,video_url,duration_seconds,status,position_x,position_y,reference_asset_ids_json,first_last_frames_json,placeholder_asset_id,placeholder_scene_asset_id,placeholder_placements_json,video_history_json,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?19)",
                    params![id,game_id,node["node_type"].as_str().unwrap_or("normal"),node["title"].as_str().unwrap_or("节点"),node["original_text"].as_str().unwrap_or(""),node["prompt"].as_str().unwrap_or(""),json_text(node.get("prompt_rich").unwrap_or(&json!([]))),node["video_url"].as_str(),duration,node["status"].as_str().unwrap_or(NOT_GENERATED),node["position_x"].as_i64().unwrap_or(80+(index as i64%4)*280),node["position_y"].as_i64().unwrap_or(80+(index as i64/4)*190),json_text(&references),json_text(&frames),placeholder,placeholder_scene,json_text(&placeholder_placements),json_text(&history),now()],
                )?;
                saved_nodes.push(json!({"id":id,"node_type":node["node_type"],"title":node["title"],"original_text":node["original_text"],"prompt":node["prompt"],"prompt_rich":node.get("prompt_rich").cloned().unwrap_or_else(||json!([])),"reference_asset_ids":references,"first_last_frames":frames,"placeholder_asset_id":placeholder,"placeholder_scene_asset_id":placeholder_scene,"placeholder_placements":placeholder_placements,"duration_seconds":duration,"status":node["status"].as_str().unwrap_or(NOT_GENERATED)}));
            }
            let mut saved_edges = Vec::new();
            for (index, edge) in edges.iter().enumerate() {
                let source = node_ids.get(edge["source_node_id"].as_str().unwrap_or_default()).cloned().unwrap_or_default();
                let target = node_ids.get(edge["target_node_id"].as_str().unwrap_or_default()).cloned().unwrap_or_default();
                let source_id = edge["id"].as_str().map(str::to_owned).unwrap_or_else(new_id);
                let id = format!("{game_id}:edge:{source_id}:{index}");
                transaction.execute(
                    "INSERT INTO game_edges (id,game_id,source_node_id,target_node_id,option_text,sort_order,conditions_json,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8)",
                    params![id,game_id,source,target,edge["option_text"].as_str().unwrap_or("继续"),edge["sort_order"].as_i64().unwrap_or(index as i64+1),json_text(edge.get("conditions").unwrap_or(&json!({}))),now()],
                )?;
                saved_edges.push(json!({"id":id,"source_node_id":source,"target_node_id":target,"option_text":edge["option_text"],"sort_order":edge["sort_order"],"conditions":edge.get("conditions").cloned().unwrap_or_else(||json!({}))}));
            }
            transaction.execute(
                "UPDATE interactive_games SET assets_json=?1,nodes_json=?2,edges_json=?3,status=?4,updated_at=?5 WHERE id=?6",
                params![json_text(&Value::Array(saved_assets)),json_text(&Value::Array(saved_nodes)),json_text(&Value::Array(saved_edges)),SUCCEEDED,now(),game_id],
            )?;
            transaction.commit()?;
            Ok(())
        })
    }

    /// Update a decomposed material's visual prompt or creator-supplied image without enqueuing a replacement image task.
    pub fn update_game_asset(
        &self,
        game_id: &str,
        asset_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            require_asset(connection, game_id, asset_id, None)?;
            for key in ["name", "prompt"] {
                if let Some(value) = values.get(key).and_then(Value::as_str) {
                    if value.trim().is_empty() {
                        return Err(AppError::BadRequest(format!("{key} 不能为空")));
                    }
                    connection.execute(&format!("UPDATE game_assets SET {key}=?1,updated_at=?2 WHERE id=?3 AND game_id=?4"), params![value.trim(),now(),asset_id,game_id])?;
                }
            }
            if let Some(url) = values.get("image_url").and_then(Value::as_str) {
                let status = if url.trim().is_empty() { NOT_GENERATED } else { "已配置" };
                connection.execute("UPDATE game_assets SET image_url=?1,status=?2,updated_at=?3 WHERE id=?4 AND game_id=?5", params![url.trim(),status,now(),asset_id,game_id])?;
            }
            if values.contains_key("voice_id") {
                let kind: String = connection.query_row("SELECT type FROM game_assets WHERE id=?1 AND game_id=?2", params![asset_id,game_id], |row| row.get(0))?;
                if kind != "character" { return Err(AppError::BadRequest("只有角色素材可以设置音色".to_owned())) }
                let voice_id = Self::normalise_voice_id(connection, values.get("voice_id"))?;
                connection.execute("UPDATE game_assets SET voice_id=?1,updated_at=?2 WHERE id=?3 AND game_id=?4", params![voice_id,now(),asset_id,game_id])?;
            }
            Ok(())
        })?;
        self.get_game_asset(game_id, asset_id)
    }

    /// Save the node prompt plus the selected reference images, first/last frames, and placeholder that a later video task must use.
    pub fn update_game_node(
        &self,
        game_id: &str,
        node_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            require_node(connection, game_id, node_id)?;
            for key in ["title", "original_text", "prompt", "video_url", "status"] {
                if let Some(value) = values.get(key).and_then(Value::as_str) {
                    connection.execute(&format!("UPDATE game_nodes SET {key}=?1,updated_at=?2 WHERE id=?3 AND game_id=?4"), params![value,now(),node_id,game_id])?;
                }
            }
            if let Some(nodes) = values.get("prompt_rich") {
                let nodes = prompt_rich_nodes(nodes, connection, game_id)?;
                connection.execute("UPDATE game_nodes SET prompt_rich_json=?1,updated_at=?2 WHERE id=?3 AND game_id=?4", params![json_text(&Value::Array(nodes)),now(),node_id,game_id])?;
            }
            for key in ["duration_seconds", "position_x", "position_y"] {
                if let Some(value) = values.get(key).and_then(Value::as_i64) {
                    if key == "duration_seconds" && !GAME_VIDEO_DURATION_RANGE.contains(&value) {
                        return Err(AppError::BadRequest("节点视频时长必须在 4 到 15 秒之间".to_owned()));
                    }
                    connection.execute(&format!("UPDATE game_nodes SET {key}=?1,updated_at=?2 WHERE id=?3 AND game_id=?4"), params![value,now(),node_id,game_id])?;
                }
            }
            if let Some(references) = values.get("reference_asset_ids") {
                let ids = reference_ids(references, connection, game_id)?;
                connection.execute("UPDATE game_nodes SET reference_asset_ids_json=?1,updated_at=?2 WHERE id=?3 AND game_id=?4", params![json_text(&json!(ids)),now(),node_id,game_id])?;
            }
            if let Some(frames) = values.get("first_last_frames") {
                let frames = frame_references(frames, connection, game_id)?;
                connection.execute("UPDATE game_nodes SET first_last_frames_json=?1,updated_at=?2 WHERE id=?3 AND game_id=?4", params![json_text(&frames),now(),node_id,game_id])?;
            }
            if values.contains_key("placeholder_asset_id") {
                let placeholder = values.get("placeholder_asset_id").and_then(Value::as_str).map(str::trim).filter(|id| !id.is_empty());
                if let Some(id) = placeholder { require_asset(connection, game_id, id, Some("placeholder"))?; }
                connection.execute("UPDATE game_nodes SET placeholder_asset_id=?1,updated_at=?2 WHERE id=?3 AND game_id=?4", params![placeholder,now(),node_id,game_id])?;
            }
            Ok(())
        })?;
        self.get_game_node(game_id, node_id)
    }

    /// Load all materials belonging to one game for aggregate retrieval, editing, and video task snapshots.
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

    /// Load all video nodes belonging to one game, including their saved references and first/last frame configuration.
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

    /// Load a single material for image-task snapshots and workbench operations.
    pub(crate) fn get_game_asset(&self, game_id: &str, asset_id: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT * FROM game_assets WHERE id=?1 AND game_id=?2",
                    params![asset_id, game_id],
                    row_to_json,
                )
                .optional()?
                .map(mapping::game_asset)
                .ok_or_else(|| AppError::NotFound(format!("Game asset not found: {asset_id}")))
        })
    }

    pub(crate) fn get_game_node(&self, game_id: &str, node_id: &str) -> AppResult<Value> {
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
}

fn complete_materials(assets: &[Value]) -> Vec<Value> {
    let mut materials = assets.to_vec();
    if !materials.iter().any(|asset| asset["type"] == "placeholder") {
        materials.push(json!({"id":"placeholder_default","type":"placeholder","name":"节点占位图","prompt":"互动游戏节点构图与站位占位图，清晰标注人物、场景和镜头留白。","status":NOT_GENERATED}));
    }
    if !materials.iter().any(|asset| asset["type"] == "cover") {
        materials.push(json!({"id":"cover_default","type":"cover","name":"互动游戏封面","prompt":"互动游戏封面视觉提示词；突出主角、核心场景与分支抉择氛围。","status":NOT_GENERATED}));
    }
    materials
}

fn mapped_asset_id(source: Option<&str>, assets: &HashMap<String, String>) -> Option<String> {
    source.and_then(|id| assets.get(id).cloned())
}

fn mapped_references(node: &Value, assets: &HashMap<String, String>) -> Value {
    let ids = node["reference_asset_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|id| mapped_asset_id(Some(id), assets))
        .collect::<Vec<_>>();
    json!(dedupe(ids))
}

fn mapped_frames(node: &Value, assets: &HashMap<String, String>) -> Value {
    let mut result = Map::new();
    for side in ["first", "last"] {
        if let Some(id) = node["first_last_frames"][side]["asset_id"]
            .as_str()
            .and_then(|id| mapped_asset_id(Some(id), assets))
        {
            result.insert(side.to_owned(), json!({"asset_id":id}));
        }
    }
    Value::Object(result)
}

fn reference_ids(
    value: &Value,
    connection: &rusqlite::Connection,
    game_id: &str,
) -> AppResult<Vec<String>> {
    let values = value
        .as_array()
        .ok_or_else(|| AppError::BadRequest("reference_asset_ids 必须是数组".to_owned()))?;
    let mut ids = Vec::new();
    for id in values
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        require_asset(connection, game_id, id, None)?;
        ids.push(id.to_owned());
    }
    Ok(dedupe(ids))
}

fn prompt_rich_nodes(
    value: &Value,
    connection: &rusqlite::Connection,
    game_id: &str,
) -> AppResult<Vec<Value>> {
    let nodes = value
        .as_array()
        .ok_or_else(|| AppError::BadRequest("prompt_rich 必须是数组".to_owned()))?;
    let mut normalized = Vec::new();
    for node in nodes {
        let Some(kind) = node["type"].as_str() else {
            return Err(AppError::BadRequest("prompt_rich 节点缺少 type".to_owned()));
        };
        if kind == "text" {
            normalized
                .push(json!({"type":"text","text":node["text"].as_str().unwrap_or_default()}));
            continue;
        }
        if kind != "reference" {
            return Err(AppError::BadRequest(
                "prompt_rich 仅支持 text 或 reference 节点".to_owned(),
            ));
        }
        let asset_id = node["asset_id"]
            .as_str()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| AppError::BadRequest("参考图缺少 asset_id".to_owned()))?;
        require_asset(connection, game_id, asset_id, None)?;
        normalized.push(json!({"type":"reference","asset_id":asset_id,"asset_type":node["asset_type"].as_str().unwrap_or("placeholder"),"label":node["label"].as_str().unwrap_or("占位图"),"image_url":node["image_url"],"mention_number":node["mention_number"]}));
    }
    Ok(normalized)
}

fn frame_references(
    value: &Value,
    connection: &rusqlite::Connection,
    game_id: &str,
) -> AppResult<Value> {
    let frames = value
        .as_object()
        .ok_or_else(|| AppError::BadRequest("first_last_frames 必须是对象".to_owned()))?;
    let mut result = Map::new();
    for side in ["first", "last"] {
        let Some(id) = frames
            .get(side)
            .and_then(|frame| frame.get("asset_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        require_asset(connection, game_id, id, None)?;
        result.insert(side.to_owned(), json!({"asset_id":id}));
    }
    Ok(Value::Object(result))
}

fn require_asset(
    connection: &rusqlite::Connection,
    game_id: &str,
    asset_id: &str,
    kind: Option<&str>,
) -> AppResult<()> {
    let found = connection
        .query_row(
            "SELECT type FROM game_assets WHERE id=?1 AND game_id=?2",
            params![asset_id, game_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    match found {
        Some(found) if kind.is_none_or(|kind| kind == found) => Ok(()),
        Some(_) => Err(AppError::BadRequest(format!("素材类型不匹配：{asset_id}"))),
        None => Err(AppError::BadRequest(format!("素材不存在：{asset_id}"))),
    }
}

fn require_node(connection: &rusqlite::Connection, game_id: &str, node_id: &str) -> AppResult<()> {
    connection
        .query_row(
            "SELECT 1 FROM game_nodes WHERE id=?1 AND game_id=?2",
            params![node_id, game_id],
            |_| Ok(()),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("Game node not found: {node_id}")))
}

fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut unique = Vec::new();
    for value in values {
        if !unique.contains(&value) {
            unique.push(value);
        }
    }
    unique
}
