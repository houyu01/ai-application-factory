//! One-time SQLite compatibility upgrades for data created by the Python application.

use rusqlite::{Connection, OptionalExtension};

use crate::{error::AppResult, value::now};

#[path = "migration_task_recovery.rs"]
mod task_recovery;

/// Upgrade existing SQLite tables before Rust repositories access their current columns.
pub(crate) fn migrate_legacy_schema(connection: &Connection) -> AppResult<()> {
    add_missing_columns(
        connection,
        "drama_shots",
        &[
            ("episode_id", "TEXT NOT NULL DEFAULT ''"),
            ("episode_sort_order", "INTEGER NOT NULL DEFAULT 1"),
            ("duration_seconds", "INTEGER NOT NULL DEFAULT 10"),
            ("prompt_rich_json", "TEXT NOT NULL DEFAULT '[]'"),
            ("placeholder_scene_asset_id", "TEXT"),
            ("placeholder_placements_json", "TEXT NOT NULL DEFAULT '[]'"),
            ("structured_json", "TEXT NOT NULL DEFAULT '{}'"),
            ("quality_json", "TEXT NOT NULL DEFAULT '{}'"),
            ("quality_status", "TEXT NOT NULL DEFAULT '未检查'"),
            ("quality_issues_json", "TEXT NOT NULL DEFAULT '[]'"),
            ("reference_asset_ids_json", "TEXT NOT NULL DEFAULT '[]'"),
            ("prompt_template_id", "TEXT"),
            ("prompt_template_version", "TEXT NOT NULL DEFAULT 'v1'"),
        ],
    )?;
    add_missing_columns(
        connection,
        "drama_assets",
        &[
            ("image_history_json", "TEXT NOT NULL DEFAULT '[]'"),
            ("content_hash", "TEXT"),
            ("source_type", "TEXT NOT NULL DEFAULT 'generated'"),
            ("variants_json", "TEXT NOT NULL DEFAULT '[]'"),
            ("metadata_json", "TEXT NOT NULL DEFAULT '{}'"),
            ("voice_id", "TEXT"),
        ],
    )?;
    // Video-version refinement fields retain the selected source and its reusable creator feedback.
    add_missing_columns(
        connection,
        "drama_shot_versions",
        &[
            ("refinement_source_version_id", "TEXT"),
            ("source_video_url", "TEXT"),
            ("refinement_prompt", "TEXT NOT NULL DEFAULT ''"),
        ],
    )?;
    migrate_refinement_prompt_ownership(connection)?;
    add_missing_columns(
        connection,
        "short_dramas",
        &[
            ("video_model", "TEXT NOT NULL DEFAULT 'doubao-seedance-2.0'"),
            ("resolution", "TEXT NOT NULL DEFAULT '720p'"),
            ("episode_count", "INTEGER NOT NULL DEFAULT 25"),
            ("enable_web_search", "INTEGER NOT NULL DEFAULT 0"),
            ("expanded_script_min_chars", "INTEGER NOT NULL DEFAULT 5000"),
            (
                "expanded_script_max_chars",
                "INTEGER NOT NULL DEFAULT 10000",
            ),
            ("shot_script_max_chars", "INTEGER NOT NULL DEFAULT 400"),
            ("video_public_prompt", "TEXT NOT NULL DEFAULT ''"),
            ("expanded_script", "TEXT NOT NULL DEFAULT ''"),
            ("asset_public_prompts_json", "TEXT NOT NULL DEFAULT '{}'"),
            ("shot_constraints_json", "TEXT NOT NULL DEFAULT '{}'"),
        ],
    )?;
    add_missing_columns(
        connection,
        "generation_tasks",
        &[
            ("job_id", "TEXT NOT NULL DEFAULT ''"),
            ("task_no", "INTEGER NOT NULL DEFAULT 1"),
            ("trigger_type", "TEXT NOT NULL DEFAULT 'GENERIC'"),
            ("input_snapshot_json", "TEXT"),
            ("output_result_json", "TEXT"),
            ("duration_ms", "INTEGER"),
            ("poll_attempts", "INTEGER NOT NULL DEFAULT 0"),
            ("poll_lease_token", "TEXT"),
            ("poll_lease_until", "TEXT"),
            ("provider_task_id", "TEXT"),
            ("progress", "INTEGER NOT NULL DEFAULT 0"),
            ("stage", "TEXT NOT NULL DEFAULT ''"),
            ("next_poll_at", "TEXT"),
            ("finished_at", "TEXT"),
        ],
    )?;
    add_missing_columns(
        connection,
        "interactive_games",
        &[("video_model", "TEXT NOT NULL DEFAULT 'doubao-seedance-2.0'")],
    )?;
    add_missing_columns(
        connection,
        "game_tasks",
        &[
            ("progress", "INTEGER NOT NULL DEFAULT 0"),
            ("stage", "TEXT NOT NULL DEFAULT ''"),
            ("poll_attempts", "INTEGER NOT NULL DEFAULT 0"),
            ("poll_lease_token", "TEXT"),
            ("poll_lease_until", "TEXT"),
            ("next_poll_at", "TEXT"),
        ],
    )?;
    task_recovery::recover_interrupted_generation_tasks(connection)?;
    recover_unclaimed_task_queue_states(connection)?;
    fold_legacy_episodes(connection)
}

/// Clear abandoned local leases so an application restart resumes the durable task queue.
fn recover_unclaimed_task_queue_states(connection: &Connection) -> AppResult<()> {
    connection.execute(
        "UPDATE generation_tasks SET stage='等待队列',poll_lease_token=NULL,poll_lease_until=NULL WHERE status='生成中' AND (stage='' OR stage IS NULL) AND provider_task_id IS NULL AND (poll_lease_until IS NULL OR poll_lease_until<?1)",
        [now()],
    )?;
    Ok(())
}

/// Remove the feedback that an earlier refinement implementation copied from a source video to its generated child.
fn migrate_refinement_prompt_ownership(connection: &Connection) -> AppResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS desktop_schema_migrations (id TEXT PRIMARY KEY, applied_at TEXT NOT NULL)",
    )?;
    let applied = connection
        .query_row(
            "SELECT 1 FROM desktop_schema_migrations WHERE id='refinement_prompt_ownership_v1'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if applied {
        return Ok(());
    }
    connection.execute(
        "UPDATE drama_shot_versions AS child SET refinement_prompt='' WHERE COALESCE(child.refinement_prompt,'')<>'' AND child.refinement_source_version_id IS NOT NULL AND EXISTS (SELECT 1 FROM drama_shot_versions AS source WHERE source.id=child.refinement_source_version_id AND source.refinement_prompt=child.refinement_prompt)",
        [],
    )?;
    connection.execute(
        "INSERT INTO desktop_schema_migrations (id,applied_at) VALUES ('refinement_prompt_ownership_v1',?1)",
        [now()],
    )?;
    Ok(())
}

fn add_missing_columns(
    connection: &Connection,
    table: &str,
    columns: &[(&str, &str)],
) -> AppResult<()> {
    let existing = connection
        .prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<std::collections::HashSet<_>, _>>()?;
    for (name, definition) in columns {
        if !existing.contains(*name) {
            connection.execute(
                &format!("ALTER TABLE {table} ADD COLUMN {name} {definition}"),
                [],
            )?;
        }
    }
    Ok(())
}

fn fold_legacy_episodes(connection: &Connection) -> AppResult<()> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='drama_episodes'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Ok(());
    }
    connection.execute_batch("UPDATE drama_shots SET episode_sort_order=COALESCE((SELECT sort_order FROM drama_episodes e WHERE e.id=drama_shots.episode_id AND e.drama_id=drama_shots.drama_id), episode_sort_order, 1), episode_name=COALESCE(NULLIF(episode_name,''),(SELECT title FROM drama_episodes e WHERE e.id=drama_shots.episode_id AND e.drama_id=drama_shots.drama_id),'第1集') WHERE EXISTS (SELECT 1 FROM drama_episodes e WHERE e.id=drama_shots.episode_id AND e.drama_id=drama_shots.drama_id); DROP INDEX IF EXISTS idx_drama_episodes_drama_id; DROP TABLE drama_episodes;")?;
    Ok(())
}
