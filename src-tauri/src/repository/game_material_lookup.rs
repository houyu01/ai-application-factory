//! Read-only interactive-game material and node lookups shared by editor and worker flows.

use rusqlite::{params, OptionalExtension};
use serde_json::Value;

use crate::{
    error::{AppError, AppResult},
    mapping,
    value::row_to_json,
};

use super::Repository;

impl Repository {
    /// Load all materials belonging to one game for aggregate retrieval, editing, and task snapshots.
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

    /// Load all video nodes belonging to one game, including saved references and frame choices.
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

    /// Load one material for image-task snapshots and workbench operations.
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

    /// Load one video node for editor operations and durable worker snapshots.
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
