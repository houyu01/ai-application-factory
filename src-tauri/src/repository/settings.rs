//! Provider, prompt-template, voice-catalog, and object-storage settings persistence.

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    value::{json_field, json_text, new_id, now, row_to_json, string},
};

use super::Repository;

impl Repository {
    /// Atomically replace all four model settings and storage after the service has probed every candidate.
    pub(crate) fn save_imported_settings(
        &self,
        models: Vec<Map<String, Value>>,
        storage: Map<String, Value>,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            let timestamp = now();
            for model in &models {
                let kind = string(model, "kind", "");
                transaction.execute(
                    "INSERT INTO app_settings (key,value_json,updated_at) VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at",
                    params![kind, json_text(&Value::Object(model.clone())), timestamp],
                )?;
            }
            transaction.execute(
                "INSERT INTO app_settings (key,value_json,updated_at) VALUES ('storage',?1,?2) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at",
                params![json_text(&Value::Object(storage)), timestamp],
            )?;
            transaction.commit()?;
            Ok(())
        })?;
        Ok(
            json!({"status":"saved","models":self.model_configs()?,"storage":self.storage_config()?}),
        )
    }

    /// List prompt templates filtered as requested by the rich-prompt editor.
    pub fn prompt_templates(
        &self,
        scope: &str,
        name: Option<&str>,
        include_inactive: bool,
    ) -> AppResult<Vec<Value>> {
        self.db.with_connection(|connection| {
            let mut query = "SELECT * FROM prompt_templates WHERE scope=?1".to_owned();
            if name.is_some() {
                query.push_str(" AND name=?2");
            }
            if !include_inactive {
                query.push_str(if name.is_some() {
                    " AND active=1"
                } else {
                    " AND active=1"
                });
            }
            query.push_str(" ORDER BY name,created_at DESC");
            let mut statement = connection.prepare(&query)?;
            let mapper = |row: &rusqlite::Row<'_>| {
                let mut item = row_to_json(row)?;
                let object = item.as_object_mut().expect("template is object");
                let metadata = json_field(object, "metadata_json", json!({}));
                object.insert("metadata".to_owned(), metadata);
                Ok(item)
            };
            let rows = if let Some(name) = name {
                statement
                    .query_map(params![scope, name], mapper)?
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                statement
                    .query_map([scope], mapper)?
                    .collect::<Result<Vec<_>, _>>()?
            };
            Ok(rows)
        })
    }

    /// Add a template version and atomically deactivate older versions of the same named template.
    pub fn create_prompt_template(&self, values: Map<String, Value>) -> AppResult<Value> {
        let scope = string(&values, "scope", "drama");
        let name = string(&values, "name", "");
        let version = string(&values, "version", "");
        let text = string(&values, "template_text", "");
        if name.is_empty() || version.is_empty() || text.is_empty() {
            return Err(AppError::BadRequest(
                "模板名称、版本和内容不能为空".to_owned(),
            ));
        }
        let id = new_id();
        let timestamp = now();
        self.db.with_connection(|connection| { let transaction = connection.unchecked_transaction()?; transaction.execute("UPDATE prompt_templates SET active=0,updated_at=?1 WHERE scope=?2 AND name=?3", params![timestamp,scope,name])?; transaction.execute("INSERT INTO prompt_templates (id,scope,name,version,template_text,metadata_json,active,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,1,?7,?7)", params![id,scope,name,version,text,json_text(values.get("metadata").unwrap_or(&json!({}))),timestamp])?; transaction.commit()?; Ok(()) })?;
        self.db.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT * FROM prompt_templates WHERE id=?1",
                    [id],
                    row_to_json,
                )
                .map_err(Into::into)
        })
    }

    /// Return object-storage settings, including the user-requested original storage credentials.
    pub fn storage_config(&self) -> AppResult<Value> {
        let config = self.setting("storage")?;
        let mut object = config.as_object().cloned().unwrap_or_default();
        object
            .entry("provider".to_owned())
            .or_insert_with(|| json!("local"));
        object
            .entry("prefix".to_owned())
            .or_insert_with(|| json!("media"));
        for key in ["secret_id", "secret_key"] {
            let configured = object
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty());
            object.insert(format!("{key}_set"), json!(configured));
        }
        Ok(Value::Object(object))
    }

    /// Persist the explicitly selected local/TOS/COS/OSS storage target while preserving omitted credentials.
    pub fn save_storage_config(&self, values: Map<String, Value>) -> AppResult<Value> {
        let stored = self.storage_config_candidate(&values)?;
        self.set_setting("storage", Value::Object(stored))?;
        self.storage_config()
    }

    /// Build the storage probe candidate with existing credentials when the settings form leaves them blank.
    pub(crate) fn storage_config_candidate(
        &self,
        values: &Map<String, Value>,
    ) -> AppResult<Map<String, Value>> {
        let provider = string(&values, "provider", "local");
        if !["local", "tos", "cos", "oss"].contains(&provider.as_str()) {
            return Err(AppError::BadRequest("不支持的存储服务商".to_owned()));
        }
        let mut stored = self
            .setting("storage")?
            .as_object()
            .cloned()
            .unwrap_or_default();
        for (key, value) in values {
            if ["secret_id", "secret_key"].contains(&key.as_str())
                && value.as_str().map(str::is_empty).unwrap_or(true)
            {
                continue;
            }
            stored.insert(key.clone(), value.clone());
        }
        stored.insert("provider".to_owned(), json!(provider));
        stored
            .entry("prefix".to_owned())
            .or_insert_with(|| json!("media"));
        Ok(stored)
    }

    /// Read a raw persisted setting for worker/provider code; API code must call the public projection instead.
    pub(crate) fn setting(&self, key: &str) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT value_json FROM app_settings WHERE key=?1",
                    [key],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_else(|| json!({}))
                .pipe(Ok)
        })
    }

    /// Persist a JSON setting with one transaction, suitable for model and storage config updates.
    pub(crate) fn set_setting(&self, key: &str, value: Value) -> AppResult<()> {
        self.db.with_connection(|connection| { connection.execute("INSERT INTO app_settings (key,value_json,updated_at) VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at", params![key,json_text(&value),now()])?; Ok(()) })
    }
}

trait Pipe: Sized {
    fn pipe<T>(self, value: impl FnOnce(Self) -> T) -> T {
        value(self)
    }
}
impl<T> Pipe for T {}
