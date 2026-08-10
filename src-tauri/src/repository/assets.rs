//! Drama asset, image-history, upload, and alternative-form persistence.

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    mapping,
    value::{json_field, json_text, new_id, now, row_to_json, string, NOT_GENERATED, SUCCEEDED},
};

use super::Repository;

impl Repository {
    /// Create an editable asset before image generation so durable jobs always have a stable resource id.
    pub fn create_asset(&self, drama_id: &str, values: Map<String, Value>) -> AppResult<Value> {
        self.assert_drama(drama_id)?;
        let kind = string(&values, "type", "");
        let name = string(&values, "name", "");
        if ![
            "character",
            "scene",
            "prop",
            "placeholder",
            "cover_reference",
            "cover",
        ]
        .contains(&kind.as_str())
        {
            return Err(AppError::BadRequest("不支持的素材类型".to_owned()));
        }
        if name.is_empty() {
            return Err(AppError::BadRequest("素材名称不能为空".to_owned()));
        }
        let id = new_id();
        let timestamp = now();
        self.db.with_connection(|connection| {
            let voice_id = Self::normalise_voice_id(connection, values.get("voice_id"))?;
            connection.execute(
                "INSERT INTO drama_assets (id,drama_id,type,name,prompt,voice_id,image_url,metadata_json,status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,NULL,?7,?8,?9,?9)",
                params![id, drama_id, kind, name, string(&values,"prompt",""), voice_id, json_text(values.get("metadata").unwrap_or(&json!({}))), NOT_GENERATED, timestamp],
            )?;
            Ok(())
        })?;
        self.get_asset(drama_id, &id)
    }

    /// Return one asset scoped to its project to avoid cross-project reference edits.
    pub fn get_asset(&self, drama_id: &str, id: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT * FROM drama_assets WHERE id=?1 AND drama_id=?2",
                    params![id, drama_id],
                    row_to_json,
                )
                .optional()?
                .map(mapping::asset)
                .ok_or_else(|| AppError::NotFound(format!("Asset not found: {id}")))
        })
    }

    /// Change an asset's user-editable fields while preserving image and task metadata.
    pub fn update_asset(
        &self,
        drama_id: &str,
        id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        self.get_asset(drama_id, id)?;
        self.db.with_connection(|connection| {
            let timestamp = now();
            if let Some(name) = values.get("name").and_then(Value::as_str) {
                if name.trim().is_empty() { return Err(AppError::BadRequest("素材名称不能为空".to_owned())); }
                connection.execute("UPDATE drama_assets SET name=?1,updated_at=?2 WHERE id=?3 AND drama_id=?4", params![name.trim(), timestamp, id, drama_id])?;
            }
            if let Some(prompt) = values.get("prompt").and_then(Value::as_str) { connection.execute("UPDATE drama_assets SET prompt=?1,updated_at=?2 WHERE id=?3 AND drama_id=?4", params![prompt, now(), id, drama_id])?; }
            if values.contains_key("voice_id") {
                let voice_id = Self::normalise_voice_id(connection, values.get("voice_id"))?;
                connection.execute("UPDATE drama_assets SET voice_id=?1,updated_at=?2 WHERE id=?3 AND drama_id=?4", params![voice_id, now(), id, drama_id])?;
            }
            if let Some(metadata) = values.get("metadata") { connection.execute("UPDATE drama_assets SET metadata_json=?1,updated_at=?2 WHERE id=?3 AND drama_id=?4", params![json_text(metadata), now(), id, drama_id])?; }
            Ok(())
        })?;
        self.get_asset(drama_id, id)
    }

    /// Persist an uploaded or generated image and append an immutable history entry for the image-history dialog.
    pub fn set_asset_image(
        &self,
        drama_id: &str,
        id: &str,
        image_url: &str,
        source_type: &str,
        status: &str,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let raw = connection.query_row("SELECT * FROM drama_assets WHERE id=?1 AND drama_id=?2", params![id, drama_id], row_to_json).optional()?
                .ok_or_else(|| AppError::NotFound(format!("Asset not found: {id}")))?;
            let mut row = raw.as_object().cloned().expect("asset row is object");
            let mut history = json_field(&mut row, "image_history_json", json!([])).as_array().cloned().unwrap_or_default();
            history.push(json!({"id": new_id(), "url": image_url, "generated_at": now(), "source_type": source_type}));
            connection.execute("UPDATE drama_assets SET image_url=?1,image_history_json=?2,source_type=?3,status=?4,updated_at=?5 WHERE id=?6 AND drama_id=?7", params![image_url, json_text(&Value::Array(history)), source_type, status, now(), id, drama_id])?;
            Ok(())
        })?;
        self.get_asset(drama_id, id)
    }

    /// Mark an asset's image task state without replacing its last usable image.
    pub fn set_asset_status(&self, drama_id: &str, id: &str, status: &str) -> AppResult<()> {
        self.db.with_connection(|connection| {
            if connection.execute(
                "UPDATE drama_assets SET status=?1,updated_at=?2 WHERE id=?3 AND drama_id=?4",
                params![status, now(), id, drama_id],
            )? == 0
            {
                return Err(AppError::NotFound(format!("Asset not found: {id}")));
            }
            Ok(())
        })
    }

    /// Delete an asset; shot reference cleanup is performed transactionally to avoid dangling prompt references.
    pub fn delete_asset(&self, drama_id: &str, id: &str) -> AppResult<Value> {
        self.get_asset(drama_id, id)?;
        self.db.with_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            transaction.execute("DELETE FROM drama_assets WHERE id=?1 AND drama_id=?2", params![id, drama_id])?;
            let mut statement = transaction.prepare("SELECT id,reference_asset_ids_json,prompt_rich_json FROM drama_shots WHERE drama_id=?1")?;
            let rows = statement.query_map([drama_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))?.collect::<Result<Vec<_>, _>>()?;
            drop(statement);
            for (shot_id, references, rich) in rows {
                let references: Vec<Value> = serde_json::from_str(&references).unwrap_or_default();
                let kept_refs: Vec<Value> = references.into_iter().filter(|item| item.as_str() != Some(id)).collect();
                let nodes: Vec<Value> = serde_json::from_str(&rich).unwrap_or_default();
                let kept_nodes: Vec<Value> = nodes.into_iter().filter(|node| node["asset_id"].as_str() != Some(id)).collect();
                transaction.execute("UPDATE drama_shots SET reference_asset_ids_json=?1,prompt_rich_json=?2,updated_at=?3 WHERE id=?4", params![json_text(&Value::Array(kept_refs)), json_text(&Value::Array(kept_nodes)), now(), shot_id])?;
            }
            transaction.commit()?;
            Ok(json!({"status":"deleted", "id":id}))
        })
    }

    /// Add an alternative form as JSON within its owning asset, preserving the established storage format.
    pub fn create_asset_variant(
        &self,
        drama_id: &str,
        asset_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        let asset = self.get_asset(drama_id, asset_id)?;
        let name = string(&values, "name", "");
        if name.is_empty() {
            return Err(AppError::BadRequest("形态名称不能为空".to_owned()));
        }
        let mut variants = asset["variants"].as_array().cloned().unwrap_or_default();
        let variant = json!({"id": new_id(), "name": name, "prompt": string(&values,"prompt",""), "image_url": Value::Null, "status": NOT_GENERATED, "image_history": []});
        variants.push(variant.clone());
        self.write_variants(drama_id, asset_id, &variants)?;
        Ok(variant)
    }

    /// Update one alternative form while retaining history and generation state.
    pub fn update_asset_variant(
        &self,
        drama_id: &str,
        asset_id: &str,
        variant_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        let asset = self.get_asset(drama_id, asset_id)?;
        let mut variants = asset["variants"].as_array().cloned().unwrap_or_default();
        let variant = variants
            .iter_mut()
            .find(|item| item["id"].as_str() == Some(variant_id))
            .ok_or_else(|| AppError::NotFound(format!("Asset variant not found: {variant_id}")))?;
        if let Some(name) = values.get("name").and_then(Value::as_str) {
            if name.trim().is_empty() {
                return Err(AppError::BadRequest("形态名称不能为空".to_owned()));
            }
            variant["name"] = json!(name.trim());
        }
        if let Some(prompt) = values.get("prompt").and_then(Value::as_str) {
            variant["prompt"] = json!(prompt);
        }
        let result = variant.clone();
        self.write_variants(drama_id, asset_id, &variants)?;
        Ok(result)
    }

    /// Remove one alternative form selected by the asset drawer.
    pub fn delete_asset_variant(
        &self,
        drama_id: &str,
        asset_id: &str,
        variant_id: &str,
    ) -> AppResult<Value> {
        let asset = self.get_asset(drama_id, asset_id)?;
        let mut variants = asset["variants"].as_array().cloned().unwrap_or_default();
        let original = variants.len();
        variants.retain(|item| item["id"].as_str() != Some(variant_id));
        if variants.len() == original {
            return Err(AppError::NotFound(format!(
                "Asset variant not found: {variant_id}"
            )));
        }
        self.write_variants(drama_id, asset_id, &variants)?;
        Ok(json!({"status":"deleted", "id":variant_id}))
    }

    /// Replace one variant after an image worker completes while appending a versioned image history entry.
    pub fn set_asset_variant_image(
        &self,
        drama_id: &str,
        asset_id: &str,
        variant_id: &str,
        image_url: &str,
        status: &str,
    ) -> AppResult<Value> {
        let asset = self.get_asset(drama_id, asset_id)?;
        let mut variants = asset["variants"].as_array().cloned().unwrap_or_default();
        let variant = variants
            .iter_mut()
            .find(|item| item["id"].as_str() == Some(variant_id))
            .ok_or_else(|| AppError::NotFound(format!("Asset variant not found: {variant_id}")))?;
        let mut history = variant["image_history"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        history.push(json!({"id":new_id(),"url":image_url,"generated_at":now()}));
        variant["image_url"] = json!(image_url);
        variant["status"] = json!(status);
        variant["image_history"] = Value::Array(history);
        let result = variant.clone();
        self.write_variants(drama_id, asset_id, &variants)?;
        Ok(result)
    }

    /// Change only a variant task state before its image URL is available.
    pub fn set_asset_variant_status(
        &self,
        drama_id: &str,
        asset_id: &str,
        variant_id: &str,
        status: &str,
    ) -> AppResult<Value> {
        let asset = self.get_asset(drama_id, asset_id)?;
        let mut variants = asset["variants"].as_array().cloned().unwrap_or_default();
        let variant = variants
            .iter_mut()
            .find(|item| item["id"].as_str() == Some(variant_id))
            .ok_or_else(|| AppError::NotFound(format!("Asset variant not found: {variant_id}")))?;
        variant["status"] = json!(status);
        let result = variant.clone();
        self.write_variants(drama_id, asset_id, &variants)?;
        Ok(result)
    }

    /// Return all public assets after a partial-refresh task requests only the changed drawer data.
    pub fn list_assets(&self, drama_id: &str) -> AppResult<Vec<Value>> {
        self.db
            .with_connection(|connection| self.assets_for(connection, drama_id))
    }

    fn write_variants(&self, drama_id: &str, asset_id: &str, variants: &[Value]) -> AppResult<()> {
        self.db.with_connection(|connection| { connection.execute("UPDATE drama_assets SET variants_json=?1,updated_at=?2 WHERE id=?3 AND drama_id=?4", params![json_text(&Value::Array(variants.to_vec())), now(), asset_id, drama_id])?; Ok(()) })
    }

    pub(crate) fn assert_drama(&self, id: &str) -> AppResult<()> {
        self.raw_drama(id).map(|_| ())
    }

    /// Normalize an optional character voice and reject IDs absent from the enabled preset catalog.
    pub(crate) fn normalise_voice_id(
        connection: &Connection,
        value: Option<&Value>,
    ) -> AppResult<Option<String>> {
        let Some(value) = value else {
            return Ok(None);
        };
        if value.is_null() {
            return Ok(None);
        }
        let value = value
            .as_str()
            .ok_or_else(|| AppError::BadRequest("voice_id 必须是字符串或 null".to_owned()))?;
        let value = value.trim();
        if value.is_empty() || value == "none" {
            return Ok(None);
        }
        let exists = connection
            .query_row(
                "SELECT 1 FROM voice_presets WHERE id=?1 AND enabled=1",
                [value],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Err(AppError::BadRequest(format!(
                "Voice preset not found: {value}"
            )));
        }
        Ok(Some(value.to_owned()))
    }

    /// Expose task-worker success state with the existing Chinese status label.
    pub(crate) fn mark_asset_succeeded(
        &self,
        drama_id: &str,
        asset_id: &str,
        url: &str,
    ) -> AppResult<Value> {
        self.set_asset_image(drama_id, asset_id, url, "generated", SUCCEEDED)
    }
}
