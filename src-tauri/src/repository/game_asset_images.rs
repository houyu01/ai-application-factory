//! Durable image-generation persistence for the interactive-game material workbench.

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    mapping,
    value::{json_text, new_id, now, row_to_json, GENERATING, NOT_GENERATED},
};

use super::Repository;

const IMAGE_KINDS: [&str; 3] = ["character", "scene", "prop"];

impl Repository {
    /// Create a creator-added reusable character, scene, or prop before it receives a durable image task.
    pub fn create_game_asset(&self, game_id: &str, values: Map<String, Value>) -> AppResult<Value> {
        let kind = required_kind(&values)?;
        let name = required_text(&values, "name")?;
        let prompt = required_text(&values, "prompt")?;
        let id = new_id();
        self.db.with_connection(|connection| {
            game_exists(connection, game_id)?;
            let voice_id = if kind == "character" {
                Self::normalise_voice_id(connection, values.get("voice_id"))?
            } else { None };
            connection.execute(
                "INSERT INTO game_assets (id,game_id,type,name,prompt,voice_id,status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8)",
                params![id, game_id, kind, name, prompt, voice_id, NOT_GENERATED, now()],
            )?;
            Ok(())
        })?;
        self.get_game_asset(game_id, &id)
    }

    /// Delete one material and clear every node control or rich-prompt chip that would otherwise point to it.
    pub fn delete_game_asset(&self, game_id: &str, asset_id: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            game_exists(connection, game_id)?;
            let found = connection
                .query_row(
                    "SELECT 1 FROM game_assets WHERE id=?1 AND game_id=?2",
                    params![asset_id, game_id],
                    |_| Ok(()),
                )
                .optional()?;
            if found.is_none() {
                return Err(AppError::NotFound(format!("Game asset not found: {asset_id}")));
            }
            let mut rows = connection.prepare("SELECT id,prompt_rich_json,reference_asset_ids_json,first_last_frames_json,placeholder_asset_id,placeholder_scene_asset_id,placeholder_placements_json FROM game_nodes WHERE game_id=?1")?;
            let nodes = rows
                .query_map([game_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, Option<String>>(4)?, row.get::<_, Option<String>>(5)?, row.get::<_, String>(6)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for (node_id, raw_prompt, raw_refs, raw_frames, placeholder, placeholder_scene, raw_placements) in nodes {
                let prompt = serde_json::from_str::<Vec<Value>>(&raw_prompt).unwrap_or_default().into_iter().filter(|node| node["asset_id"].as_str() != Some(asset_id)).collect::<Vec<_>>();
                let refs = serde_json::from_str::<Vec<String>>(&raw_refs)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|id| id != asset_id)
                    .collect::<Vec<_>>();
                let mut frames = serde_json::from_str::<Map<String, Value>>(&raw_frames).unwrap_or_default();
                frames.retain(|_, frame| frame["asset_id"].as_str() != Some(asset_id));
                let placeholder = placeholder.filter(|id| id != asset_id);
                let placeholder_scene = placeholder_scene.filter(|id| id != asset_id);
                let placements = serde_json::from_str::<Vec<Value>>(&raw_placements)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|placement| placement["asset_id"].as_str() != Some(asset_id))
                    .collect::<Vec<_>>();
                connection.execute(
                    "UPDATE game_nodes SET prompt_rich_json=?1,reference_asset_ids_json=?2,first_last_frames_json=?3,placeholder_asset_id=?4,placeholder_scene_asset_id=?5,placeholder_placements_json=?6,updated_at=?7 WHERE id=?8",
                    params![json_text(&json!(prompt)), json_text(&json!(refs)), json_text(&Value::Object(frames)), placeholder, placeholder_scene, json_text(&json!(placements)), now(), node_id],
                )?;
            }
            connection.execute("DELETE FROM game_assets WHERE id=?1 AND game_id=?2", params![asset_id, game_id])?;
            Ok(json!({"status":"deleted","id":asset_id}))
        })
    }

    /// Persist the shared generation prompt for one material type without changing individual creator prompts.
    pub fn update_game_asset_public_prompt(
        &self,
        game_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        let kind = required_kind(&values)?;
        let prompt = values
            .get("public_prompt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        self.db.with_connection(|connection| {
            let raw = connection
                .query_row("SELECT asset_public_prompts_json FROM interactive_games WHERE id=?1", [game_id], |row| row.get::<_, String>(0))
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("Interactive game not found: {game_id}")))?;
            let mut prompts = serde_json::from_str::<Map<String, Value>>(&raw).unwrap_or_default();
            prompts.insert(kind.to_owned(), json!(prompt));
            connection.execute(
                "UPDATE interactive_games SET asset_public_prompts_json=?1,updated_at=?2 WHERE id=?3",
                params![json_text(&Value::Object(prompts)), now(), game_id],
            )?;
            Ok(())
        })?;
        self.get_game(game_id)
    }

    /// Create or reuse an asset image task after freezing the prompt and configured global visual direction.
    pub fn enqueue_game_asset_image(&self, game_id: &str, asset_id: &str) -> AppResult<Value> {
        let asset = self.get_game_asset(game_id, asset_id)?;
        self.enqueue_game_image_task(game_id, asset_id, None, &asset)
    }

    /// Queue all current assets of one type independently so refreshes and failures remain isolated per card.
    pub fn enqueue_game_asset_images(&self, game_id: &str, kind: &str) -> AppResult<Vec<Value>> {
        if !IMAGE_KINDS.contains(&kind) {
            return Err(AppError::BadRequest("不支持的素材类型".to_owned()));
        }
        let assets = self.db.with_connection(|connection| {
            game_exists(connection, game_id)?;
            let mut statement = connection.prepare(
                "SELECT * FROM game_assets WHERE game_id=?1 AND type=?2 ORDER BY created_at,id",
            )?;
            let assets = statement
                .query_map(params![game_id, kind], row_to_json)?
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(mapping::game_asset)
                .collect::<Vec<_>>();
            Ok(assets)
        })?;
        assets
            .iter()
            .map(|asset| {
                self.enqueue_game_image_task(
                    game_id,
                    asset["id"].as_str().unwrap_or_default(),
                    None,
                    asset,
                )
            })
            .collect()
    }

    /// Add an independent pose, outfit, or state beneath a reusable material.
    pub fn create_game_asset_variant(
        &self,
        game_id: &str,
        asset_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        let name = required_text(&values, "name")?;
        let prompt = required_text(&values, "prompt")?;
        let id = new_id();
        self.edit_game_variants(game_id, asset_id, |variants| {
            variants.push(json!({"id":id,"name":name,"prompt":prompt,"image_url":null,"image_history":[],"status":NOT_GENERATED,"created_at":now(),"updated_at":now()}));
            Ok(())
        })?;
        self.get_game_asset_variant(game_id, asset_id, &id)
    }

    /// Save the name or prompt of one alternate material form without changing its generated images.
    pub fn update_game_asset_variant(
        &self,
        game_id: &str,
        asset_id: &str,
        variant_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        self.edit_game_variants(game_id, asset_id, |variants| {
            let variant = find_variant_mut(variants, variant_id)?;
            for key in ["name", "prompt"] {
                if let Some(value) = values.get(key).and_then(Value::as_str) {
                    let value = value.trim();
                    if value.is_empty() {
                        return Err(AppError::BadRequest(format!("{key} 不能为空")));
                    }
                    variant[key] = json!(value);
                }
            }
            variant["updated_at"] = json!(now());
            Ok(())
        })?;
        self.get_game_asset_variant(game_id, asset_id, variant_id)
    }

    /// Remove one alternate form without affecting the base material image or other forms.
    pub fn delete_game_asset_variant(
        &self,
        game_id: &str,
        asset_id: &str,
        variant_id: &str,
    ) -> AppResult<Value> {
        self.edit_game_variants(game_id, asset_id, |variants| {
            let previous = variants.len();
            variants.retain(|variant| variant["id"].as_str() != Some(variant_id));
            if previous == variants.len() {
                return Err(AppError::NotFound(format!(
                    "Game asset variant not found: {variant_id}"
                )));
            }
            Ok(())
        })?;
        Ok(json!({"status":"deleted","id":variant_id}))
    }

    /// Create or reuse an image task for one alternate material form.
    pub fn enqueue_game_asset_variant_image(
        &self,
        game_id: &str,
        asset_id: &str,
        variant_id: &str,
    ) -> AppResult<Value> {
        let asset = self.get_game_asset(game_id, asset_id)?;
        let variant = asset["variants"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item["id"].as_str() == Some(variant_id))
            })
            .cloned()
            .ok_or_else(|| {
                AppError::NotFound(format!("Game asset variant not found: {variant_id}"))
            })?;
        self.enqueue_game_image_task(
            game_id,
            asset_id,
            Some(variant_id),
            &json!({"asset":asset,"variant":variant}),
        )
    }

    /// Persist the provider result as the current base image and append a recoverable history record.
    pub fn finish_game_asset_image(
        &self,
        game_id: &str,
        asset_id: &str,
        task_id: &str,
        url: &str,
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            let raw = connection.query_row("SELECT image_history_json FROM game_assets WHERE id=?1 AND game_id=?2", params![asset_id, game_id], |row| row.get::<_, String>(0))?;
            let mut history = serde_json::from_str::<Vec<Value>>(&raw).unwrap_or_default();
            history.push(json!({"id":task_id,"url":url,"generated_at":now(),"task_id":task_id,"status":"生成成功"}));
            connection.execute("UPDATE game_assets SET image_url=?1,image_history_json=?2,status='生成成功',updated_at=?3 WHERE id=?4 AND game_id=?5", params![url,json_text(&json!(history)),now(),asset_id,game_id])?;
            Ok(())
        })
    }

    /// Persist one alternate-form image result while retaining its prior images in variant history.
    pub fn finish_game_asset_variant_image(
        &self,
        game_id: &str,
        asset_id: &str,
        variant_id: &str,
        task_id: &str,
        url: &str,
    ) -> AppResult<()> {
        self.edit_game_variants(game_id, asset_id, |variants| {
            let variant = find_variant_mut(variants, variant_id)?;
            let mut history = variant["image_history"].as_array().cloned().unwrap_or_default();
            history.push(json!({"id":task_id,"url":url,"generated_at":now(),"task_id":task_id,"status":"生成成功"}));
            variant["image_url"] = json!(url);
            variant["image_history"] = json!(history);
            variant["status"] = json!("生成成功");
            variant["updated_at"] = json!(now());
            Ok(())
        })
    }

    /// Mirror a failed image task only to the affected base material or alternate form.
    pub fn set_game_asset_image_status(
        &self,
        game_id: &str,
        asset_id: &str,
        variant_id: Option<&str>,
        status: &str,
    ) -> AppResult<()> {
        if let Some(variant_id) = variant_id {
            return self.edit_game_variants(game_id, asset_id, |variants| {
                let variant = find_variant_mut(variants, variant_id)?;
                variant["status"] = json!(status);
                variant["updated_at"] = json!(now());
                Ok(())
            });
        }
        self.db.with_connection(|connection| {
            if connection.execute(
                "UPDATE game_assets SET status=?1,updated_at=?2 WHERE id=?3 AND game_id=?4",
                params![status, now(), asset_id, game_id],
            )? == 0
            {
                return Err(AppError::NotFound(format!(
                    "Game asset not found: {asset_id}"
                )));
            }
            Ok(())
        })
    }

    fn enqueue_game_image_task(
        &self,
        game_id: &str,
        asset_id: &str,
        variant_id: Option<&str>,
        snapshot_asset: &Value,
    ) -> AppResult<Value> {
        let kind = if variant_id.is_some() {
            "game_asset_variant_image"
        } else {
            "game_asset_image"
        };
        let task = self.db.with_connection(|connection| {
            game_exists(connection, game_id)?;
            let existing = connection.query_row("SELECT * FROM game_tasks WHERE game_id=?1 AND type=?2 AND resource_id=?3 AND status=?4 ORDER BY created_at DESC LIMIT 1", params![game_id,kind,variant_id.unwrap_or(asset_id),GENERATING], row_to_json).optional()?;
            if let Some(task) = existing { return Ok(mapping::game_task(task)); }
            let id = new_id();
            let snapshot = if let Some(variant) = variant_id { json!({"game_id":game_id,"asset_id":asset_id,"variant_id":variant,"asset":snapshot_asset["asset"],"variant":snapshot_asset["variant"]}) } else { json!({"game_id":game_id,"asset_id":asset_id,"asset":snapshot_asset}) };
            connection.execute("INSERT INTO game_tasks (id,game_id,type,resource_id,status,input_snapshot_json,progress,stage,created_at,started_at) VALUES (?1,?2,?3,?4,?5,?6,0,'等待素材图片生成',?7,?7)", params![id,game_id,kind,variant_id.unwrap_or(asset_id),GENERATING,json_text(&snapshot),now()])?;
            Ok(mapping::game_task(connection.query_row("SELECT * FROM game_tasks WHERE id=?1", [id], row_to_json)?))
        })?;
        self.set_game_asset_image_status(game_id, asset_id, variant_id, GENERATING)?;
        Ok(task)
    }

    fn edit_game_variants(
        &self,
        game_id: &str,
        asset_id: &str,
        edit: impl FnOnce(&mut Vec<Value>) -> AppResult<()>,
    ) -> AppResult<()> {
        self.db.with_connection(|connection| {
            let raw = connection
                .query_row(
                    "SELECT variants_json FROM game_assets WHERE id=?1 AND game_id=?2",
                    params![asset_id, game_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| AppError::NotFound(format!("Game asset not found: {asset_id}")))?;
            let mut variants = serde_json::from_str::<Vec<Value>>(&raw).unwrap_or_default();
            edit(&mut variants)?;
            connection.execute(
                "UPDATE game_assets SET variants_json=?1,updated_at=?2 WHERE id=?3 AND game_id=?4",
                params![json_text(&json!(variants)), now(), asset_id, game_id],
            )?;
            Ok(())
        })
    }

    fn get_game_asset_variant(
        &self,
        game_id: &str,
        asset_id: &str,
        variant_id: &str,
    ) -> AppResult<Value> {
        self.get_game_asset(game_id, asset_id)?["variants"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item["id"].as_str() == Some(variant_id))
            })
            .cloned()
            .ok_or_else(|| {
                AppError::NotFound(format!("Game asset variant not found: {variant_id}"))
            })
    }
}

fn required_kind(values: &Map<String, Value>) -> AppResult<&str> {
    let kind = values
        .get("asset_type")
        .or_else(|| values.get("type"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if IMAGE_KINDS.contains(&kind) {
        Ok(kind)
    } else {
        Err(AppError::BadRequest("不支持的素材类型".to_owned()))
    }
}

fn required_text(values: &Map<String, Value>, key: &str) -> AppResult<String> {
    let value = values
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if value.is_empty() {
        Err(AppError::BadRequest(format!("{key} 不能为空")))
    } else {
        Ok(value.to_owned())
    }
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

fn find_variant_mut<'a>(variants: &'a mut [Value], variant_id: &str) -> AppResult<&'a mut Value> {
    variants
        .iter_mut()
        .find(|variant| variant["id"].as_str() == Some(variant_id))
        .ok_or_else(|| AppError::NotFound(format!("Game asset variant not found: {variant_id}")))
}
