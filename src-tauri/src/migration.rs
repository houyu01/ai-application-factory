//! One-time SQLite compatibility upgrades for data created by the Python application.

use rusqlite::{Connection, OptionalExtension};

use crate::{
    error::AppResult,
    value::now,
    volcengine_tts::{apply_seed_tts_two_defaults, migrate_legacy_async_profile},
};

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
            // One completed video per shot can be selected as the default ZIP-export source.
            ("is_selected_for_export", "INTEGER NOT NULL DEFAULT 0"),
        ],
    )?;
    migrate_refinement_prompt_ownership(connection)?;
    migrate_legacy_ark_tts_settings(connection)?;
    add_missing_columns(
        connection,
        "short_dramas",
        &[
            ("video_model", "TEXT NOT NULL DEFAULT 'doubao-seedance-2.0'"),
            ("resolution", "TEXT NOT NULL DEFAULT '720p'"),
            ("episode_count", "INTEGER NOT NULL DEFAULT 15"),
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
        &[
            ("video_model", "TEXT NOT NULL DEFAULT 'doubao-seedance-2.0'"),
            ("expanded_script", "TEXT NOT NULL DEFAULT ''"),
            ("resolution", "TEXT NOT NULL DEFAULT '720p'"),
            ("enable_web_search", "INTEGER NOT NULL DEFAULT 0"),
            ("expanded_script_min_chars", "INTEGER NOT NULL DEFAULT 5000"),
            (
                "expanded_script_max_chars",
                "INTEGER NOT NULL DEFAULT 10000",
            ),
            ("node_script_max_chars", "INTEGER NOT NULL DEFAULT 400"),
            ("asset_public_prompts_json", "TEXT NOT NULL DEFAULT '{}'"),
        ],
    )?;
    // Interactive-game assets share the same durable image history and alternative-form boundary as short-drama assets.
    add_missing_columns(
        connection,
        "game_assets",
        &[
            ("image_history_json", "TEXT NOT NULL DEFAULT '[]'"),
            ("variants_json", "TEXT NOT NULL DEFAULT '[]'"),
            ("metadata_json", "TEXT NOT NULL DEFAULT '{}'"),
            // Character materials can select a catalog voice used as a video audio reference.
            ("voice_id", "TEXT"),
        ],
    )?;
    add_missing_columns(connection, "voice_presets", &[("audio_url", "TEXT")])?;
    add_missing_columns(
        connection,
        "game_nodes",
        &[
            ("prompt_rich_json", "TEXT NOT NULL DEFAULT '[]'"),
            // Keeps the short-drama-compatible multi-shot / long-shot prompt choice per game node.
            ("prompt_template_version", "TEXT NOT NULL DEFAULT 'v1'"),
            ("reference_asset_ids_json", "TEXT NOT NULL DEFAULT '[]'"),
            ("first_last_frames_json", "TEXT NOT NULL DEFAULT '{}'"),
            ("placeholder_asset_id", "TEXT"),
            ("placeholder_scene_asset_id", "TEXT"),
            ("placeholder_placements_json", "TEXT NOT NULL DEFAULT '[]'"),
            // Completed history ID chosen by the creator for the editor and playable runtime.
            ("selected_video_id", "TEXT"),
        ],
    )?;
    // Runtime sessions retain early-choice flags after paths merge into a shared video node.
    add_missing_columns(
        connection,
        "game_sessions",
        &[("state_json", "TEXT NOT NULL DEFAULT '{}'")],
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
            ("provider_task_id", "TEXT"),
        ],
    )?;
    migrate_game_video_duration_range(connection)?;
    task_recovery::recover_interrupted_generation_tasks(connection)?;
    // Voice previews use a synchronous configured TTS call; restart returns any orphaned local lease to its queue.
    connection.execute(
        "UPDATE voice_generation_tasks SET stage='等待生成音色',progress=0 WHERE status='生成中' AND stage='正在生成音色'",
        [],
    )?;
    recover_unclaimed_task_queue_states(connection)?;
    fold_legacy_episodes(connection)
}

/// Upgrade persisted audio profiles from the retired `/api/v1/tts_async` protocol to Seed-TTS 2.0.
fn migrate_legacy_ark_tts_settings(connection: &Connection) -> AppResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS desktop_schema_migrations (id TEXT PRIMARY KEY, applied_at TEXT NOT NULL)",
    )?;
    let applied = connection
        .query_row(
            "SELECT 1 FROM desktop_schema_migrations WHERE id='seed_tts_2_v2'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if applied {
        return Ok(());
    }
    let value = connection
        .query_row(
            "SELECT value_json FROM app_settings WHERE key='audio'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(value) = value else {
        connection.execute(
            "INSERT INTO desktop_schema_migrations (id,applied_at) VALUES ('seed_tts_2_v2',?1)",
            [now()],
        )?;
        return Ok(());
    };
    let mut value = serde_json::from_str::<serde_json::Value>(&value).unwrap_or_default();
    let mut changed = false;
    if let Some(audio) = value.as_object_mut() {
        if audio.get("provider").and_then(serde_json::Value::as_str) == Some("ark") {
            if !migrate_legacy_async_profile(audio) {
                apply_seed_tts_two_defaults(audio);
            }
            changed = true;
        }
        if let Some(profiles) = audio
            .get_mut("provider_profiles")
            .and_then(serde_json::Value::as_object_mut)
        {
            if let Some(ark) = profiles
                .get_mut("ark")
                .and_then(serde_json::Value::as_object_mut)
            {
                if !migrate_legacy_async_profile(ark) {
                    apply_seed_tts_two_defaults(ark);
                }
                changed = true;
            }
        }
    }
    if changed {
        connection.execute(
            "UPDATE app_settings SET value_json=?1,updated_at=?2 WHERE key='audio'",
            [value.to_string(), now()],
        )?;
    }
    connection.execute(
        "INSERT INTO desktop_schema_migrations (id,applied_at) VALUES ('seed_tts_2_v2',?1)",
        [now()],
    )?;
    Ok(())
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

/// Repair early interactive-game nodes that allowed 3-second or over-15-second videos.
///
/// The persisted values are read when a durable node task is submitted. Normalizing both the
/// project defaults and existing nodes keeps resumed tasks within Ark Seedance 2.0's 4–15 second
/// request range without changing any prompt, reference, or completed video.
fn migrate_game_video_duration_range(connection: &Connection) -> AppResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS desktop_schema_migrations (id TEXT PRIMARY KEY, applied_at TEXT NOT NULL)",
    )?;
    let applied = connection
        .query_row(
            "SELECT 1 FROM desktop_schema_migrations WHERE id='game_video_duration_range_v1'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if applied {
        return Ok(());
    }
    let timestamp = now();
    connection.execute(
        "UPDATE interactive_games SET node_duration_min=MIN(15,MAX(4,node_duration_min)),node_duration_max=MIN(15,MAX(4,node_duration_max)),updated_at=?1",
        [&timestamp],
    )?;
    connection.execute(
        "UPDATE interactive_games SET node_duration_max=node_duration_min,updated_at=?1 WHERE node_duration_max<node_duration_min",
        [&timestamp],
    )?;
    connection.execute(
        "UPDATE game_nodes SET duration_seconds=MIN(15,MAX(4,duration_seconds)),updated_at=?1",
        [&timestamp],
    )?;
    connection.execute(
        "INSERT INTO desktop_schema_migrations (id,applied_at) VALUES ('game_video_duration_range_v1',?1)",
        [&timestamp],
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
