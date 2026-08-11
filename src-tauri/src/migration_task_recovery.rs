//! Startup recovery for durable tasks whose in-process model calls ended with the previous app.

use std::collections::BTreeSet;

use rusqlite::{params, Connection};
use serde_json::Value;

use crate::{
    error::AppResult,
    value::{json_text, now, FAILED, GENERATING},
};

const INTERRUPTED_STAGE: &str = "应用重启后任务中断";
const INTERRUPTED_ERROR: &str = "应用重启后无法恢复此任务，请重试。";
const VIDEO_RETRY_STAGE: &str = "应用重启后无法恢复视频任务";
const VIDEO_RETRY_ERROR: &str = "应用重启前未保存远端视频任务，无法继续查询，请重试生成。";
const GAME_INTERRUPTED_STAGE: &str = "应用重启后游戏生成中断";
const GAME_INTERRUPTED_ERROR: &str = "应用重启后无法恢复互动游戏生成，请重试。";

/// Reconcile durable work on desktop startup: remote video jobs resume polling, while interrupted local work becomes retryable.
pub(crate) fn recover_interrupted_generation_tasks(connection: &Connection) -> AppResult<()> {
    let timestamp = now();
    fail_interrupted_local_tasks(connection, &timestamp)?;
    fail_untracked_video_tasks(connection, &timestamp)?;
    resume_video_polls(connection, &timestamp)?;
    mark_interrupted_assets_failed(connection, &timestamp)?;
    mark_interrupted_variants_failed(connection)?;
    mark_interrupted_screenplays_failed(connection, &timestamp)?;
    mark_interrupted_game_generation_failed(connection, &timestamp)?;
    Ok(())
}

fn mark_interrupted_game_generation_failed(
    connection: &Connection,
    timestamp: &str,
) -> AppResult<()> {
    connection.execute(
        "UPDATE game_tasks SET status=?1,stage=?2,error_message=?3,progress=100,completed_at=?4,poll_lease_token=NULL,poll_lease_until=NULL WHERE status=?5 AND type IN ('game_script_expansion','game_graph_decomposition')",
        params![FAILED, GAME_INTERRUPTED_STAGE, GAME_INTERRUPTED_ERROR, timestamp, GENERATING],
    )?;
    connection.execute(
        "UPDATE interactive_games SET status=?1,updated_at=?2 WHERE id IN (SELECT game_id FROM game_tasks WHERE status=?1 AND stage=?3 AND completed_at=?2)",
        params![FAILED, timestamp, GAME_INTERRUPTED_STAGE],
    )?;
    Ok(())
}

fn fail_interrupted_local_tasks(connection: &Connection, timestamp: &str) -> AppResult<()> {
    connection.execute(
        "UPDATE generation_tasks SET status=?1,stage=?2,error_message=?3,completed_at=?4,finished_at=?4,next_poll_at=NULL,poll_lease_token=NULL,poll_lease_until=NULL WHERE status=?5 AND type NOT IN ('shot_video','serial_shot_video_batch') AND COALESCE(stage,'') NOT IN ('','等待队列','等待重试')",
        params![FAILED, INTERRUPTED_STAGE, INTERRUPTED_ERROR, timestamp, GENERATING],
    )?;
    Ok(())
}

fn fail_untracked_video_tasks(connection: &Connection, timestamp: &str) -> AppResult<()> {
    connection.execute(
        "UPDATE generation_tasks SET status=?1,stage=?2,error_message=?3,completed_at=?4,finished_at=?4,next_poll_at=NULL,poll_lease_token=NULL,poll_lease_until=NULL WHERE status=?5 AND type='shot_video' AND COALESCE(provider_task_id,'')='' AND COALESCE(stage,'') NOT IN ('','等待队列','等待重试')",
        params![FAILED, VIDEO_RETRY_STAGE, VIDEO_RETRY_ERROR, timestamp, GENERATING],
    )?;
    connection.execute(
        "UPDATE drama_shot_versions SET status=?1,error_message=?2,progress=100,completed_at=?3 WHERE task_id IN (SELECT id FROM generation_tasks WHERE status=?1 AND stage=?4 AND type='shot_video') AND status=?5",
        params![FAILED, VIDEO_RETRY_ERROR, timestamp, VIDEO_RETRY_STAGE, GENERATING],
    )?;
    Ok(())
}

fn resume_video_polls(connection: &Connection, timestamp: &str) -> AppResult<()> {
    connection.execute(
        "UPDATE generation_tasks SET stage='正在恢复视频任务轮询',next_poll_at=?1,poll_lease_token=NULL,poll_lease_until=NULL WHERE status=?2 AND type='shot_video' AND COALESCE(provider_task_id,'')<>''",
        params![timestamp, GENERATING],
    )?;
    Ok(())
}

fn mark_interrupted_assets_failed(connection: &Connection, timestamp: &str) -> AppResult<()> {
    connection.execute(
        "UPDATE drama_assets SET status=?1,updated_at=?2 WHERE status=?3 AND id IN (SELECT resource_id FROM generation_tasks WHERE status=?1 AND stage=?4 AND type IN ('asset_image','cover_image','placeholder_image'))",
        params![FAILED, timestamp, GENERATING, INTERRUPTED_STAGE],
    )?;
    Ok(())
}

fn mark_interrupted_variants_failed(connection: &Connection) -> AppResult<()> {
    let ids = interrupted_variant_ids(connection)?;
    if ids.is_empty() {
        return Ok(());
    }
    let assets = asset_variants(connection)?;
    for (asset_id, raw_variants) in assets {
        let mut variants = serde_json::from_str::<Vec<Value>>(&raw_variants).unwrap_or_default();
        let mut changed = false;
        for variant in &mut variants {
            if ids.contains(variant["id"].as_str().unwrap_or_default()) {
                variant["status"] = Value::String(FAILED.to_owned());
                changed = true;
            }
        }
        if changed {
            connection.execute(
                "UPDATE drama_assets SET variants_json=?1 WHERE id=?2",
                params![json_text(&Value::Array(variants)), asset_id],
            )?;
        }
    }
    Ok(())
}

fn interrupted_variant_ids(connection: &Connection) -> AppResult<BTreeSet<String>> {
    let mut statement = connection.prepare(
        "SELECT resource_id FROM generation_tasks WHERE status=?1 AND stage=?2 AND type='asset_variant_image'",
    )?;
    let rows = statement.query_map(params![FAILED, INTERRUPTED_STAGE], |row| {
        row.get::<_, String>(0)
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn asset_variants(connection: &Connection) -> AppResult<Vec<(String, String)>> {
    let mut statement = connection.prepare("SELECT id,variants_json FROM drama_assets")?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn mark_interrupted_screenplays_failed(connection: &Connection, timestamp: &str) -> AppResult<()> {
    connection.execute(
        "UPDATE short_dramas SET status=?1,updated_at=?2 WHERE id IN (SELECT drama_id FROM generation_tasks WHERE status=?1 AND stage=?3 AND type='script_decomposition')",
        params![FAILED, timestamp, INTERRUPTED_STAGE],
    )?;
    Ok(())
}
