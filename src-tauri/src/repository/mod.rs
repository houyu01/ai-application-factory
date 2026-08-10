//! SQLite repository facade. It is the only Rust layer that issues compatibility SQL.

mod assets;
mod game_validation;
mod games;
mod project_list;
mod projects;
mod settings;
mod settings_models;
mod shots;
mod tasks;
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
