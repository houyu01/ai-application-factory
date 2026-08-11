//! SQLite repository facade. It is the only Rust layer that issues compatibility SQL.

mod assets;
mod game_asset_images;
mod game_covers;
mod game_frame_references;
mod game_generation_checkpoints;
mod game_graph_validation;
mod game_material_lookup;
mod game_materials;
mod game_placeholders;
mod game_prompt;
mod game_reference_images;
mod game_regeneration;
mod game_state;
pub(crate) mod game_validation;
mod game_video_history;
mod game_video_tasks;
mod game_workflows;
mod games;
mod project_list;
mod project_validation;
mod projects;
mod settings;
mod settings_models;
mod shots;
mod tasks;
mod video_exports;
mod voice_presets;

pub(crate) use shots::{ShotVersionInput, ShotVideoRefinement};

use crate::db::Database;

/// Owns local persistence; services coordinate rules while this facade owns every SQLite transaction.
#[derive(Clone)]
pub struct Repository {
    pub(crate) db: Database,
}

impl Repository {
    /// Bind the local application repository to an already initialized SQLite database.
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
