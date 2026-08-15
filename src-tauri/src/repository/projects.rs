//! Short-drama project persistence and aggregate detail projections.

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    mapping, planner,
    value::{json_text, new_id, now, row_to_json, string, GENERATING, NOT_GENERATED, SUCCEEDED},
};

use super::{
    project_validation::{create_integer, optional_boolean},
    Repository,
};

impl Repository {
    /// Persist a drama and its durable bootstrap task atomically before a worker sees it.
    pub fn create_drama(&self, values: Map<String, Value>) -> AppResult<Value> {
        let name = string(&values, "name", "");
        let script = string(&values, "script", "");
        if script.chars().count() < 10 {
            return Err(AppError::BadRequest("剧本文本不少于 10 个字".to_owned()));
        }
        let episode_count = create_integer(&values, "episode_count", 15, 2, 100)?;
        let enable_web_search = optional_boolean(&values, "enable_web_search")?.unwrap_or(false);
        let minimum = create_integer(&values, "expanded_script_min_chars", 5_000, 1, 1_000_000)?;
        let maximum = create_integer(&values, "expanded_script_max_chars", 10_000, 1, 1_000_000)?;
        let shot_limit = create_integer(&values, "shot_script_max_chars", 400, 1, 1_000_000)?;
        if minimum > maximum {
            return Err(AppError::BadRequest(
                "扩写字数最小值不能大于最大值".to_owned(),
            ));
        }
        let id = new_id();
        let task_id = new_id();
        let timestamp = now();
        let snapshot = json!({
            "drama_id": id, "script": script, "language_model": string(&values, "language_model", "doubao-seed"),
            "episode_count": episode_count, "enable_web_search": enable_web_search,
            "expanded_script_min_chars": minimum, "expanded_script_max_chars": maximum,
            "shot_script_max_chars": shot_limit,
        });
        self.db.with_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            transaction.execute(
                "INSERT INTO short_dramas (id,name,script,ratio,style,theme,language_model,multimodal_model,video_model,episode_count,enable_web_search,expanded_script_min_chars,expanded_script_max_chars,shot_script_max_chars,resolution,video_public_prompt,asset_public_prompts_json,shot_constraints_json,status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?20)",
                params![id, name, script, string(&values,"ratio","9:16"), string(&values,"style","真人风格"), string(&values,"theme","都市"), string(&values,"language_model","doubao-seed"), string(&values,"multimodal_model","doubao-seeddream"), string(&values,"video_model","doubao-seedance-2.0"), episode_count, enable_web_search as i64, minimum, maximum, shot_limit, string(&values,"resolution","720p"), string(&values,"video_public_prompt",""), json_text(values.get("asset_public_prompts").unwrap_or(&json!({}))), json_text(values.get("shot_constraints").unwrap_or(&json!({}))), GENERATING, timestamp],
            )?;
            transaction.execute(
                "INSERT INTO generation_tasks (id,drama_id,type,job_id,task_no,trigger_type,status,input_snapshot_json,progress,stage,created_at,started_at) VALUES (?1,?2,'script_decomposition',?2,1,'DRAMA_BOOTSTRAP',?3,?4,0,'',?5,?5)",
                params![task_id, id, GENERATING, json_text(&snapshot), timestamp],
            )?;
            transaction.commit()?;
            Ok(())
        })?;
        let mut project = self.get_drama(&id)?;
        let object = project.as_object_mut().expect("project is object");
        object.insert("task_id".to_owned(), json!(task_id));
        object.insert("task".to_owned(), self.get_drama_task(&task_id)?);
        Ok(project)
    }

    /// Return one complete editor project after verifying that it exists in local SQLite.
    pub fn get_drama(&self, id: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let raw = connection
                .query_row("SELECT * FROM short_dramas WHERE id=?1", [id], row_to_json)
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("Project not found: {id}")))?;
            let mut project = mapping::drama(raw);
            let assets = self.assets_for(connection, id)?;
            let mut shots = self.shots_for(connection, id)?;
            self.attach_shot_versions(connection, id, &mut shots)?;
            let tasks = self.drama_tasks_for(connection, id, true)?;
            let object = project.as_object_mut().expect("project is object");
            object.insert("assets".to_owned(), Value::Array(assets));
            object.insert("shots".to_owned(), Value::Array(shots.clone()));
            object.insert(
                "episodes".to_owned(),
                Value::Array(mapping::episodes(&shots)),
            );
            object.insert("tasks".to_owned(), Value::Array(tasks));
            let queued = object["tasks"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter(|task| task["status"] == GENERATING)
                        .count()
                })
                .unwrap_or(0);
            object.insert("generation_queue".to_owned(), json!({"running": queued}));
            Ok(project)
        })
    }

    /// Build the editor response with one selected full shot and the persisted expanded screenplay for its banner.
    pub fn get_editor_drama(&self, id: &str, selected_shot_id: Option<&str>) -> AppResult<Value> {
        let mut project = self.get_drama(id)?;
        let expanded_script = self.get_expanded_screenplay(id)?["expanded_script"].clone();
        let object = project.as_object_mut().expect("project is object");
        let shots = object["shots"].as_array().cloned().unwrap_or_default();
        let selected = selected_shot_id
            .filter(|value| shots.iter().any(|shot| shot["id"].as_str() == Some(*value)))
            .or_else(|| shots.first().and_then(|shot| shot["id"].as_str()))
            .unwrap_or_default()
            .to_owned();
        object.insert("script".to_owned(), json!(""));
        object.insert("expanded_script".to_owned(), expanded_script);
        object.insert("historical_videos".to_owned(), json!([]));
        object.insert(
            "shots".to_owned(),
            Value::Array(
                shots
                    .into_iter()
                    .map(|shot| editor_shot(shot, &selected))
                    .collect(),
            ),
        );
        Ok(project)
    }

    /// Rename or update explicit global project parameters without replacing omitted values.
    pub fn update_drama(&self, id: &str, values: Map<String, Value>) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            if values.is_empty() {
                return Err(AppError::BadRequest("没有可保存的项目字段".to_owned()));
            }
            let mut set = vec!["updated_at=?".to_owned()];
            let mut parameters: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now())];
            for (key, column) in [
                ("name", "name"),
                ("ratio", "ratio"),
                ("style", "style"),
                ("theme", "theme"),
                ("resolution", "resolution"),
                ("language_model", "language_model"),
                ("multimodal_model", "multimodal_model"),
                ("video_model", "video_model"),
                ("video_public_prompt", "video_public_prompt"),
            ] {
                if let Some(value) = values.get(key).and_then(Value::as_str) {
                    if value.trim().is_empty() {
                        return Err(AppError::BadRequest(format!("{key} 不能为空")));
                    }
                    set.push(format!("{column}=?"));
                    parameters.push(Box::new(value.to_owned()));
                }
            }
            if let Some(value) = optional_boolean(&values, "enable_web_search")? {
                set.push("enable_web_search=?".to_owned());
                parameters.push(Box::new(value as i64));
            }
            for (key, column) in [
                ("asset_public_prompts", "asset_public_prompts_json"),
                ("shot_constraints", "shot_constraints_json"),
            ] {
                if let Some(value) = values.get(key) {
                    set.push(format!("{column}=?"));
                    parameters.push(Box::new(json_text(value)));
                }
            }
            if set.len() == 1 {
                return Err(AppError::BadRequest("没有可保存的项目字段".to_owned()));
            }
            parameters.push(Box::new(id.to_owned()));
            let query = format!("UPDATE short_dramas SET {} WHERE id=?", set.join(","));
            if connection.execute(
                &query,
                rusqlite::params_from_iter(parameters.iter().map(|item| item.as_ref())),
            )? == 0
            {
                return Err(AppError::NotFound(format!("Project not found: {id}")));
            }
            Ok(())
        })?;
        self.get_drama(id)
    }

    /// Save original and expanded screenplay text from the screenplay editor without regenerating shots.
    pub fn update_screenplay(&self, id: &str, values: Map<String, Value>) -> AppResult<Value> {
        let script = string(&values, "script", "");
        if script.chars().count() < 10 {
            return Err(AppError::BadRequest("剧本文本不少于 10 个字".to_owned()));
        }
        self.db.with_connection(|connection| {
            if connection.execute(
                "UPDATE short_dramas SET script=?1, expanded_script=?2, updated_at=?3 WHERE id=?4",
                params![script, string(&values, "expanded_script", ""), now(), id],
            )? == 0
            {
                return Err(AppError::NotFound(format!("Project not found: {id}")));
            }
            Ok(())
        })?;
        self.get_expanded_screenplay(id)
    }

    /// Return the full screenplay only for the dedicated modal endpoint.
    pub fn get_expanded_screenplay(&self, id: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT id,script,expanded_script,updated_at FROM short_dramas WHERE id=?1",
                    [id],
                    row_to_json,
                )
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("Project not found: {id}")))
        })
    }

    /// Delete a complete drama graph; foreign keys remove assets, shots, versions, and durable tasks.
    pub fn delete_drama(&self, id: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            if connection.execute("DELETE FROM short_dramas WHERE id=?1", [id])? == 0 {
                return Err(AppError::NotFound(format!("Project not found: {id}")));
            }
            Ok(json!({"status":"deleted", "id":id}))
        })
    }

    /// Change a project status only from the durable worker after it has persisted all affected records.
    pub fn set_drama_status(&self, id: &str, status: &str) -> AppResult<()> {
        self.db.with_connection(|connection| {
            connection.execute(
                "UPDATE short_dramas SET status=?1,updated_at=?2 WHERE id=?3",
                params![status, now(), id],
            )?;
            Ok(())
        })
    }

    /// Replace bootstrap assets and shots only after a complete plan has been produced by the durable worker.
    pub fn save_drama_decomposition(&self, drama_id: &str, plan: &Value) -> AppResult<()> {
        self.db.with_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            let project = transaction.query_row("SELECT * FROM short_dramas WHERE id=?1", [drama_id], row_to_json).optional()?
                .ok_or_else(|| AppError::NotFound(format!("Project not found: {drama_id}")))?;
            let project = mapping::drama(project);
            transaction.execute("DELETE FROM drama_assets WHERE drama_id=?1", [drama_id])?;
            transaction.execute("DELETE FROM drama_shots WHERE drama_id=?1", [drama_id])?;
            let timestamp = now();
            let mut assets = Vec::new();
            for (index, source) in plan["assets"].as_array().unwrap_or(&Vec::new()).iter().enumerate() {
                let id = format!("{drama_id}:asset:{}:{index}", source["id"].as_str().unwrap_or(&new_id()));
                let voice_id = Self::normalise_voice_id(&transaction, source.get("voice_id"))?;
                let variants = source["variants"].as_array().into_iter().flatten().enumerate().filter_map(|(variant_index, variant)| {
                    let name = variant["name"].as_str()?.trim();
                    (!name.is_empty()).then(|| json!({"id":format!("{id}:variant:{}:{variant_index}",variant["id"].as_str().unwrap_or("form")),"name":name,"prompt":variant["prompt"].as_str().unwrap_or(""),"episode_numbers":variant["episode_numbers"],"image_url":Value::Null,"image_history":[],"status":NOT_GENERATED}))
                }).collect::<Vec<_>>();
                let item = json!({"id":id,"type":source["type"].as_str().unwrap_or("prop"),"name":source["name"].as_str().unwrap_or("素材"),"prompt":source["prompt"].as_str().unwrap_or(""),"voice_id":voice_id,"variants":variants,"status":NOT_GENERATED});
                transaction.execute("INSERT INTO drama_assets (id,drama_id,type,name,prompt,voice_id,variants_json,metadata_json,status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,'{}',?8,?9,?9)", params![item["id"].as_str(),drama_id,item["type"].as_str(),item["name"].as_str(),item["prompt"].as_str(),item["voice_id"].as_str(),json_text(&item["variants"]),NOT_GENERATED,timestamp])?;
                assets.push(item);
            }
            let mut saved_shots = Vec::new();
            for (episode_index, episode) in plan["episodes"].as_array().unwrap_or(&Vec::new()).iter().enumerate() {
                let episode_id = format!("{drama_id}:episode:{}", episode_index + 1);
                let episode_name = episode["name"].as_str().unwrap_or("第1集");
                for (shot_index, source) in episode["shots"].as_array().unwrap_or(&Vec::new()).iter().enumerate() {
                    let id = format!("{drama_id}:shot:{}:{}", episode_index + 1, shot_index + 1);
                    let original = source["original_text"].as_str().unwrap_or_default();
                    let duration = source["duration_seconds"].as_i64().unwrap_or(10).clamp(3,15);
                    let draft = json!({"original_text":original,"duration_seconds":duration,"prompt_template_version":"v1"});
                    let nodes = planner::fallback_rich_prompt_with_requests(&project, &draft, &assets, source["references"].as_array().map(Vec::as_slice).unwrap_or(&[]));
                    let prompt = planner::prompt_text(&nodes);
                    let references = nodes.iter().filter(|node| node["type"] == "reference").filter_map(|node| node["asset_id"].as_str().map(str::to_owned)).collect::<Vec<_>>();
                    transaction.execute("INSERT INTO drama_shots (id,drama_id,episode_id,episode_name,episode_sort_order,shot_index,title,original_text,duration_seconds,prompt,prompt_rich_json,reference_asset_ids_json,status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?14)", params![id,drama_id,episode_id,episode_name,(episode_index+1) as i64,(shot_index+1) as i64,source["title"].as_str().unwrap_or("分镜"),original,duration,prompt,json_text(&Value::Array(nodes)),json_text(&json!(references)),NOT_GENERATED,timestamp])?;
                    saved_shots.push(json!({"id":id,"episode_id":episode_id,"episode_name":episode_name,"episode_sort_order":episode_index+1,"shot_index":shot_index+1,"title":source["title"],"original_text":original,"duration_seconds":duration,"prompt":prompt}));
                }
            }
            transaction.execute("UPDATE short_dramas SET assets_json=?1,shots_json=?2,status=?3,updated_at=?4 WHERE id=?5", params![json_text(&Value::Array(assets)),json_text(&Value::Array(saved_shots)),SUCCEEDED,now(),drama_id])?;
            transaction.commit()?;
            Ok(())
        })
    }

    /// Persist a screenplay checkpoint while a long-running expansion task is still in progress.
    pub fn set_expanded_screenplay(&self, id: &str, expanded_script: &str) -> AppResult<()> {
        self.db.with_connection(|connection| {
            connection.execute(
                "UPDATE short_dramas SET expanded_script=?1,updated_at=?2 WHERE id=?3",
                params![expanded_script, now(), id],
            )?;
            Ok(())
        })
    }

    pub(crate) fn assets_for(
        &self,
        connection: &rusqlite::Connection,
        id: &str,
    ) -> AppResult<Vec<Value>> {
        let mut statement = connection.prepare(
            "SELECT * FROM drama_assets WHERE drama_id=?1 ORDER BY created_at DESC,id DESC",
        )?;
        let rows = statement
            .query_map([id], row_to_json)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(mapping::asset)
            .collect();
        Ok(rows)
    }

    pub(crate) fn shots_for(
        &self,
        connection: &rusqlite::Connection,
        id: &str,
    ) -> AppResult<Vec<Value>> {
        let mut statement = connection.prepare("SELECT * FROM drama_shots WHERE drama_id=?1 ORDER BY episode_sort_order,episode_name,shot_index,created_at")?;
        let rows = statement
            .query_map([id], row_to_json)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(mapping::shot)
            .collect();
        Ok(rows)
    }

    /// Attach lightweight version history to each shot without opening new database connections.
    pub(crate) fn attach_shot_versions(
        &self,
        connection: &rusqlite::Connection,
        drama_id: &str,
        shots: &mut [Value],
    ) -> AppResult<()> {
        let mut statement = connection.prepare(
            "SELECT * FROM drama_shot_versions WHERE drama_id=?1 ORDER BY shot_id,version_no DESC",
        )?;
        let rows = statement
            .query_map([drama_id], row_to_json)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(mapping::shot_version);
        let mut by_shot = std::collections::BTreeMap::<String, Vec<Value>>::new();
        for version in rows {
            let shot = version["shot_id"].as_str().unwrap_or_default().to_owned();
            by_shot.entry(shot).or_default().push(version);
        }
        for shot in shots {
            let id = shot["id"].as_str().unwrap_or_default();
            shot["versions"] = Value::Array(by_shot.remove(id).unwrap_or_default());
        }
        Ok(())
    }

    pub(crate) fn drama_tasks_for(
        &self,
        connection: &rusqlite::Connection,
        id: &str,
        detail: bool,
    ) -> AppResult<Vec<Value>> {
        let mut statement = connection
            .prepare("SELECT * FROM generation_tasks WHERE drama_id=?1 ORDER BY created_at")?;
        let rows = statement
            .query_map([id], row_to_json)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|row| {
                if detail {
                    mapping::drama_detail_task(row)
                } else {
                    mapping::drama_task(row)
                }
            })
            .collect())
    }

    /// Get only the raw project record for worker logic that must access the stored expanded screenplay.
    pub(crate) fn raw_drama(&self, id: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            connection
                .query_row("SELECT * FROM short_dramas WHERE id=?1", [id], row_to_json)
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("Project not found: {id}")))
        })
    }
}

fn editor_shot(shot: Value, selected: &str) -> Value {
    let versions = shot["versions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(editor_version)
        .collect::<Vec<_>>();
    if shot["id"].as_str() == Some(selected) {
        let mut detail = shot;
        detail["versions"] = Value::Array(versions);
        return detail;
    }
    let text = shot["original_text"].as_str().unwrap_or_default();
    let preview: String = text.chars().take(600).collect();
    json!({
        "id":shot["id"], "episode_id":shot["episode_id"], "episode_name":shot["episode_name"],
        "episode_sort_order":shot["episode_sort_order"], "shot_index":shot["shot_index"],
        "title":shot["title"], "duration_seconds":shot["duration_seconds"], "status":shot["status"],
        "quality_status":shot["quality_status"],
        "original_text":format!("{}{}", preview, if text.chars().count() > 600 { "…" } else { "" }),
        "prompt":"", "prompt_rich":[], "historical_videos":[], "versions":versions,
    })
}

fn editor_version(version: &Value) -> Value {
    let mut projected = Map::new();
    for key in [
        "id",
        "version_no",
        "task_id",
        "status",
        "provider_task_id",
        "progress",
        "refinement_prompt",
        "video_url",
        "error_message",
        "is_selected_for_export",
        "created_at",
        "completed_at",
    ] {
        projected.insert(
            key.to_owned(),
            version.get(key).cloned().unwrap_or(Value::Null),
        );
    }
    Value::Object(projected)
}
