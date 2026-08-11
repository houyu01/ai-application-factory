//! Persist generated rich prompts and their reference dependencies for interactive-game nodes.

use serde_json::{json, Value};

use crate::{
    error::AppResult,
    value::{json_text, now},
};

use super::Repository;

impl Repository {
    /// Save the prompt worker's normalized node document and derive the visible reference list from its chips.
    pub(crate) fn save_generated_game_node_prompt(
        &self,
        game_id: &str,
        node_id: &str,
        prompt: &str,
        nodes: &[Value],
        template_version: &str,
    ) -> AppResult<Value> {
        self.db.with_connection(|connection| {
            let nodes = super::game_materials::prompt_rich_nodes(&json!(nodes), connection, game_id)?;
            let references = nodes.iter().filter(|node| node["type"] == "reference")
                .filter_map(|node| node["asset_id"].as_str())
                .fold(Vec::<String>::new(), |mut ids, id| {
                    if !ids.iter().any(|item| item == id) { ids.push(id.to_owned()); }
                    ids
                });
            connection.execute(
                "UPDATE game_nodes SET prompt=?1,prompt_rich_json=?2,reference_asset_ids_json=?3,prompt_template_version=?4,updated_at=?5 WHERE id=?6 AND game_id=?7",
                rusqlite::params![prompt, json_text(&Value::Array(nodes)), json_text(&json!(references)), template_version, now(), node_id, game_id],
            )?;
            Ok(())
        })?;
        self.get_game_node(game_id, node_id)
    }
}
