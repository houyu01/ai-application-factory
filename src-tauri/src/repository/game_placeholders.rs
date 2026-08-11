//! Durable interactive-game placeholder layouts, composite assets, and image-task records.

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    mapping,
    value::{json_text, new_id, now, row_to_json, GENERATING, NOT_GENERATED},
};

use super::Repository;

impl Repository {
    /// Save a node's editable scene and character layout without starting model work.
    pub fn save_game_placeholder_layout(
        &self,
        game_id: &str,
        node_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        if values
            .get("node_id")
            .and_then(Value::as_str)
            .is_some_and(|id| id != node_id)
        {
            return Err(AppError::BadRequest("请求节点与地址不匹配".to_owned()));
        }
        let scene_id = required_id(&values, "scene_asset_id", "请选择场景素材")?;
        let placements = normalized_placements(values.get("placements"))?;
        self.db.with_connection(|connection| {
            game_exists(connection, game_id)?;
            require_asset_type(connection, game_id, &scene_id, "scene")?;
            for placement in &placements {
                require_asset_type(
                    connection,
                    game_id,
                    placement["asset_id"].as_str().unwrap_or_default(),
                    "character",
                )?;
            }
            if connection.execute(
                "UPDATE game_nodes SET placeholder_scene_asset_id=?1,placeholder_placements_json=?2,updated_at=?3 WHERE id=?4 AND game_id=?5",
                params![scene_id, json_text(&json!(placements)), now(), node_id, game_id],
            )? == 0 {
                return Err(AppError::NotFound(format!("Game node not found: {node_id}")));
            }
            Ok(Value::Null)
        })?;
        self.get_game_node(game_id, node_id)
    }

    /// Insert a generated-composite placeholder asset and its durable image task in one SQLite transaction.
    pub fn enqueue_game_placeholder(
        &self,
        game_id: &str,
        node_id: &str,
        prompt: &str,
        metadata: Value,
    ) -> AppResult<Value> {
        let asset_id = new_id();
        let task_id = new_id();
        let timestamp = now();
        self.db.with_connection(|connection| {
            game_exists(connection, game_id)?;
            require_node(connection, game_id, node_id)?;
            if let Some(task) = connection
                .query_row(
                    "SELECT * FROM game_tasks WHERE game_id=?1 AND type='game_placeholder_image' AND resource_id=?2 AND status=?3 ORDER BY created_at DESC LIMIT 1",
                    params![game_id, node_id, GENERATING],
                    row_to_json,
                )
                .optional()?
            {
                let mut task = mapping::game_task(task);
                task.as_object_mut()
                    .expect("game task is an object")
                    .insert("_reused".to_owned(), json!(true));
                return Ok(json!({"task":task}));
            }
            let mut metadata = metadata.as_object().cloned().unwrap_or_default();
            metadata.insert(
                "version".to_owned(),
                json!(placeholder_version(connection, game_id, node_id)? + 1),
            );
            metadata.insert("render_mode".to_owned(), json!("generated_composite"));
            let metadata = Value::Object(metadata);
            let title = node_title(connection, game_id, node_id)?;
            let transaction = connection.unchecked_transaction()?;
            transaction.execute(
                "INSERT INTO game_assets (id,game_id,type,name,prompt,metadata_json,status,created_at,updated_at) VALUES (?1,?2,'placeholder',?3,?4,?5,?6,?7,?7)",
                params![asset_id, game_id, format!("{title} · 占位图 {}", metadata["version"].as_i64().unwrap_or(1)), prompt, json_text(&metadata), GENERATING, timestamp],
            )?;
            transaction.execute(
                "INSERT INTO game_tasks (id,game_id,type,resource_id,status,input_snapshot_json,progress,stage,created_at,started_at) VALUES (?1,?2,'game_placeholder_image',?3,?4,?5,0,'等待占位图生成',?6,?6)",
                params![task_id, game_id, node_id, GENERATING, json_text(&json!({"game_id":game_id,"node_id":node_id,"asset_id":asset_id,"scene_asset_id":metadata["scene_asset_id"],"placements":metadata["placements"],"reference_asset_ids":metadata["reference_asset_ids"],"render_mode":"generated_composite"})), timestamp],
            )?;
            transaction.commit()?;
            Ok(Value::Null)
        })?;
        Ok(json!({
            "placeholder": self.get_game_asset(game_id, &asset_id)?,
            "task": self.get_game_task(&task_id)?,
        }))
    }

    /// Attach a completed composite to its node while retaining the editable source layout for the next revision.
    pub fn apply_game_placeholder_to_node(
        &self,
        game_id: &str,
        node_id: &str,
        asset_id: &str,
        metadata: &Value,
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            require_asset_type(connection, game_id, asset_id, "placeholder")?;
            if connection.execute(
                "UPDATE game_nodes SET placeholder_asset_id=?1,placeholder_scene_asset_id=?2,placeholder_placements_json=?3,status=?4,updated_at=?5 WHERE id=?6 AND game_id=?7",
                params![asset_id, metadata["scene_asset_id"].as_str(), json_text(&metadata["placements"]), NOT_GENERATED, now(), node_id, game_id],
            )? == 0 {
                return Err(AppError::NotFound(format!("Game node not found: {node_id}")));
            }
            Ok(())
        })
    }
}

fn required_id(values: &Map<String, Value>, key: &str, message: &str) -> AppResult<String> {
    values
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| AppError::BadRequest(message.to_owned()))
}

fn normalized_placements(raw: Option<&Value>) -> AppResult<Vec<Value>> {
    let items = raw
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::BadRequest("placements 必须是数组".to_owned()))?;
    let mut placements = Vec::new();
    for (index, item) in items.iter().take(30).enumerate() {
        let asset_id = item["asset_id"]
            .as_str()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| AppError::BadRequest("占位框缺少角色素材".to_owned()))?;
        let width = number(&item["width"], 0.2).clamp(0.04, 1.0);
        let height = number(&item["height"], 0.35).clamp(0.04, 1.0);
        placements.push(json!({
            "id":item["id"].as_str().unwrap_or(&format!("placement_{}", index + 1)),
            "asset_id":asset_id,
            "x":number(&item["x"], 0.28).clamp(0.0, 1.0 - width),
            "y":number(&item["y"], 0.26).clamp(0.0, 1.0 - height),
            "width":width,
            "height":height,
            "pose":item["pose"].as_str().unwrap_or(""),
            "note":item["note"].as_str().or_else(|| item["pose"].as_str()).unwrap_or(""),
        }));
    }
    Ok(placements)
}

fn number(value: &Value, default: f64) -> f64 {
    value.as_f64().unwrap_or(default)
}

fn placeholder_version(
    connection: &rusqlite::Connection,
    game_id: &str,
    node_id: &str,
) -> AppResult<usize> {
    let mut statement = connection
        .prepare("SELECT metadata_json FROM game_assets WHERE game_id=?1 AND type='placeholder'")?;
    let rows = statement.query_map([game_id], |row| row.get::<_, String>(0))?;
    Ok(rows
        .filter_map(Result::ok)
        .filter_map(|raw| serde_json::from_str::<Value>(&raw).ok())
        .filter(|metadata| {
            metadata["node_id"].as_str() == Some(node_id)
                && metadata["render_mode"].as_str() == Some("generated_composite")
        })
        .count())
}

fn node_title(
    connection: &rusqlite::Connection,
    game_id: &str,
    node_id: &str,
) -> AppResult<String> {
    connection
        .query_row(
            "SELECT title FROM game_nodes WHERE id=?1 AND game_id=?2",
            params![node_id, game_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("Game node not found: {node_id}")))
}

fn game_exists(connection: &rusqlite::Connection, game_id: &str) -> AppResult<()> {
    connection
        .query_row(
            "SELECT 1 FROM interactive_games WHERE id=?1",
            [game_id],
            |_| Ok(()),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound(format!("Interactive game not found: {game_id}")))
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

fn require_asset_type(
    connection: &rusqlite::Connection,
    game_id: &str,
    asset_id: &str,
    expected: &str,
) -> AppResult<()> {
    let kind = connection
        .query_row(
            "SELECT type FROM game_assets WHERE id=?1 AND game_id=?2",
            params![asset_id, game_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or_else(|| AppError::BadRequest(format!("素材不存在：{asset_id}")))?;
    if kind == expected {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!("素材类型不匹配：{asset_id}")))
    }
}
