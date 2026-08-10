//! Compact project-library projections and durable bootstrap queue metadata.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};

use crate::{
    error::AppResult,
    mapping,
    value::{row_to_json, GENERATING},
};

use super::Repository;

impl Repository {
    /// Return compact project-library records before an editor has selected a project.
    pub fn list_dramas(&self) -> AppResult<Vec<Value>> {
        self.db.with_connection(|connection| {
            let queue = project_generation_queue(connection)?;
            let mut statement =
                connection.prepare("SELECT * FROM short_dramas ORDER BY created_at DESC")?;
            let dramas = statement
                .query_map([], row_to_json)?
                .collect::<Result<Vec<_>, _>>()?;
            dramas
                .into_iter()
                .map(|row| {
                    let mut drama = mapping::drama(row);
                    let id = drama["id"].as_str().unwrap_or_default().to_owned();
                    let shots = self.shots_for(connection, &id)?;
                    let assets = self.assets_for(connection, &id)?;
                    let object = drama.as_object_mut().expect("project is an object");
                    object.insert("script".to_owned(), json!(""));
                    object.insert(
                        "shots".to_owned(),
                        Value::Array(shots.iter().map(|shot| json!({"id":shot["id"]})).collect()),
                    );
                    object.insert(
                        "assets".to_owned(),
                        Value::Array(assets.iter().map(project_card_asset).collect()),
                    );
                    object.insert(
                        "episodes".to_owned(),
                        Value::Array(mapping::episodes(&shots)),
                    );
                    if let Some(state) = queue.get(&id) {
                        object.extend(state.clone());
                    }
                    Ok(drama)
                })
                .collect()
        })
    }

    /// Return the persisted queue card fields for a just-created project response.
    pub(crate) fn drama_queue_metadata(&self, drama_id: &str) -> AppResult<Map<String, Value>> {
        self.db.with_connection(|connection| {
            Ok(project_generation_queue(connection)?
                .remove(drama_id)
                .unwrap_or_default())
        })
    }
}

fn project_card_asset(asset: &Value) -> Value {
    json!({
        "id":asset["id"], "type":asset["type"], "image_url":asset["image_url"],
        "image_history":if asset["type"] == "cover" { asset["image_history"].clone() } else { json!([]) },
        "created_at":asset["created_at"],
    })
}

fn project_generation_queue(
    connection: &rusqlite::Connection,
) -> AppResult<BTreeMap<String, Map<String, Value>>> {
    let mut statement = connection.prepare("SELECT * FROM generation_tasks WHERE type='script_decomposition' AND status=?1 ORDER BY created_at,id")?;
    let tasks = statement
        .query_map([GENERATING], row_to_json)?
        .collect::<Result<Vec<_>, _>>()?;
    let now = Utc::now();
    let mut result = BTreeMap::new();
    for (index, task) in tasks.into_iter().enumerate() {
        let lease = task["poll_lease_until"]
            .as_str()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc));
        let processing = lease.is_some_and(|value| value > now)
            || task["progress"].as_i64().unwrap_or(0) > 0
            || task["stage"]
                .as_str()
                .is_some_and(|value| !value.trim().is_empty());
        let project_id = task["drama_id"].as_str().unwrap_or_default().to_owned();
        result.insert(
            project_id,
            Map::from_iter([
                ("queue_position".to_owned(), json!(index + 1)),
                (
                    "queue_state".to_owned(),
                    json!(if processing { "processing" } else { "queued" }),
                ),
            ]),
        );
    }
    Ok(result)
}
