//! Public JSON projections that retain the response shape consumed by the existing TypeScript UI.

use serde_json::{json, Map, Value};

use crate::value::json_field;

fn move_json(object: &mut Map<String, Value>, public: &str, stored: &str, default: Value) {
    let value = json_field(object, stored, default);
    object.insert(public.to_owned(), value);
}

/// Project list/detail projection; the full expanded screenplay stays behind its dedicated route.
pub fn drama(mut row: Value) -> Value {
    let object = row.as_object_mut().expect("database rows are objects");
    object.remove("expanded_script");
    move_json(object, "shots", "shots_json", json!([]));
    move_json(object, "assets", "assets_json", json!([]));
    move_json(
        object,
        "historical_videos",
        "historical_videos_json",
        json!([]),
    );
    move_json(
        object,
        "asset_public_prompts",
        "asset_public_prompts_json",
        json!({}),
    );
    move_json(
        object,
        "shot_constraints",
        "shot_constraints_json",
        json!({}),
    );
    object
        .entry("episodes".to_owned())
        .or_insert_with(|| json!([]));
    row
}

/// Asset projection including image history, alternative forms, and metadata used by specialized editors.
pub fn asset(mut row: Value) -> Value {
    let object = row.as_object_mut().expect("database rows are objects");
    move_json(object, "image_history", "image_history_json", json!([]));
    move_json(object, "variants", "variants_json", json!([]));
    move_json(object, "metadata", "metadata_json", json!({}));
    row
}

/// Shot projection with legacy duration alias and JSON editor fields restored.
pub fn shot(mut row: Value) -> Value {
    let object = row.as_object_mut().expect("database rows are objects");
    let duration = object
        .get("duration_seconds")
        .and_then(Value::as_i64)
        .unwrap_or(10)
        .clamp(3, 15);
    object.insert("duration_seconds".to_owned(), json!(duration));
    object.insert("duration".to_owned(), json!(duration));
    move_json(
        object,
        "historical_videos",
        "historical_videos_json",
        json!([]),
    );
    move_json(object, "prompt_rich", "prompt_rich_json", json!([]));
    move_json(
        object,
        "placeholder_placements",
        "placeholder_placements_json",
        json!([]),
    );
    let structured = json_field(object, "structured_json", json!({}));
    let frames = structured
        .get("first_last_frames")
        .cloned()
        .unwrap_or_else(|| json!({}));
    object.insert("structured".to_owned(), structured);
    object.insert("first_last_frames".to_owned(), frames);
    move_json(object, "quality", "quality_json", json!({}));
    move_json(object, "quality_issues", "quality_issues_json", json!([]));
    move_json(
        object,
        "reference_asset_ids",
        "reference_asset_ids_json",
        json!([]),
    );
    row
}

/// Video-version projection restores the rich prompt and review snapshots retained for version history.
pub fn shot_version(mut row: Value) -> Value {
    let object = row.as_object_mut().expect("database rows are objects");
    move_json(object, "prompt_rich", "prompt_rich_json", json!([]));
    move_json(object, "structured", "structured_json", json!({}));
    move_json(object, "quality", "quality_json", json!({}));
    row
}

/// Project task projection maps the persistence foreign key to the established provider-neutral API name.
pub fn drama_task(mut row: Value) -> Value {
    let object = row.as_object_mut().expect("database rows are objects");
    let project_id = object.get("drama_id").cloned().unwrap_or(Value::Null);
    object.insert("project_id".to_owned(), project_id);
    let input = json_field(object, "input_snapshot_json", Value::Null);
    let output = object
        .remove("output_result_json")
        .or_else(|| object.remove("result_json"));
    let result = output
        .and_then(|value| {
            value
                .as_str()
                .and_then(|text| serde_json::from_str(text).ok())
        })
        .unwrap_or(Value::Null);
    object.insert("input_snapshot".to_owned(), input);
    object.insert("result".to_owned(), result);
    row
}

/// Limit the serial-video coordinator state exposed to the editor while keeping its complete shot list internal to SQLite.
pub(crate) fn serial_video_batch_snapshot(input: &Value) -> Value {
    let mut snapshot = Map::new();
    for key in [
        "mode",
        "total_count",
        "next_index",
        "completed_count",
        "current_task_id",
        "current_shot_id",
    ] {
        if let Some(value) = input.get(key) {
            snapshot.insert(key.to_owned(), value.clone());
        }
    }
    Value::Object(snapshot)
}

/// Detail task projection retains recovery checkpoints while the editor displays screenplay text from the project.
pub fn drama_detail_task(row: Value) -> Value {
    let mut task = drama_task(row);
    let object = task.as_object_mut().expect("task is an object");
    let input_snapshot = object.get("input_snapshot").cloned().unwrap_or(Value::Null);
    let detail_input = input_snapshot
        .as_object()
        .map(|input| {
            let mut detail = Map::new();
            for key in ["shot_id", "expanded_script_preview", "story_bible_preview"] {
                if let Some(value) = input.get(key) {
                    detail.insert(key.to_owned(), value.clone());
                }
            }
            Value::Object(detail)
        })
        .unwrap_or(Value::Null);
    object.insert("input_snapshot".to_owned(), detail_input);
    if object.get("type").and_then(Value::as_str) == Some("serial_shot_video_batch") {
        let snapshot = serial_video_batch_snapshot(&input_snapshot);
        object.insert("input_snapshot".to_owned(), snapshot);
    } else if object.get("type").and_then(Value::as_str) == Some("script_decomposition") {
        let result = object
            .get("result")
            .and_then(Value::as_object)
            .map(|result| {
                let mut selected = Map::new();
                for key in ["original_script_length", "expanded_script_length"] {
                    if let Some(value) = result.get(key) {
                        selected.insert(key.to_owned(), value.clone());
                    }
                }
                Value::Object(selected)
            })
            .unwrap_or(Value::Null);
        object.insert("result".to_owned(), result);
    } else {
        object.insert("result".to_owned(), Value::Null);
    }
    task
}

/// Game task projection has the same public task field name despite a game-specific foreign key.
pub fn game_task(mut row: Value) -> Value {
    let object = row.as_object_mut().expect("database rows are objects");
    let project_id = object.get("game_id").cloned().unwrap_or(Value::Null);
    object.insert("project_id".to_owned(), project_id);
    move_json(object, "input_snapshot", "input_snapshot_json", Value::Null);
    move_json(object, "result", "result_json", Value::Null);
    row
}

/// Game asset projection currently has no JSON columns but keeps a single mapping boundary.
pub fn game_asset(row: Value) -> Value {
    row
}

/// Game node projection restores persisted video history for the graph editor.
pub fn game_node(mut row: Value) -> Value {
    let object = row.as_object_mut().expect("database rows are objects");
    move_json(object, "video_history", "video_history_json", json!([]));
    row
}

/// Game edge projection restores optional runtime condition rules.
pub fn game_edge(mut row: Value) -> Value {
    let object = row.as_object_mut().expect("database rows are objects");
    move_json(object, "conditions", "conditions_json", json!({}));
    row
}

/// Group normalised shots into the legacy episode object collection used by the list and detail pages.
pub fn episodes(shots: &[Value]) -> Vec<Value> {
    let mut grouped: std::collections::BTreeMap<(i64, String), Value> =
        std::collections::BTreeMap::new();
    for shot in shots {
        let id = shot["episode_id"]
            .as_str()
            .filter(|value| !value.is_empty())
            .unwrap_or("episode:1")
            .to_owned();
        let order = shot["episode_sort_order"].as_i64().unwrap_or(1).max(1);
        let entry = grouped.entry((order, id.clone())).or_insert_with(|| {
            json!({
                "id": id,
                "sort_order": order,
                "title": shot["episode_name"].as_str().unwrap_or("第1集"),
                "shot_count": 0,
            })
        });
        entry["shot_count"] = json!(entry["shot_count"].as_i64().unwrap_or(0) + 1);
    }
    grouped.into_values().collect()
}
