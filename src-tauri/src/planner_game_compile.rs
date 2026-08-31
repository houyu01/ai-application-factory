//! Compile a playable interactive-game DAG from a structured branch screenplay.

use std::collections::HashSet;

use serde_json::{json, Value};

use crate::{repository::game_validation::GAME_VIDEO_DURATION_RANGE, value::NOT_GENERATED};

use super::compile_graph::wire_playable_edges;
use super::screenplay::{parse_game_screenplay, ScreenplayBeat, ScreenplayIr};
use super::{integer, playable_game_plan};

/// Build a playable DAG from Sxx/Exx/前往. Extra screenplay branches may be dropped; dead ends are repaired.
pub(crate) fn compile_game_plan(screenplay: &str, game: &Value) -> Option<Value> {
    let ir = parse_game_screenplay(screenplay)?;
    let draft = compile_draft(&ir, game);
    playable_game_plan(&draft.to_string(), game)
}

fn compile_draft(ir: &ScreenplayIr, game: &Value) -> Value {
    let duration = duration_seconds(game);
    let maximum = integer(
        game,
        "branch_max",
        4,
        integer(game, "branch_min", 2, 2, 4),
        4,
    ) as usize;
    let mut beats = ir.beats.clone();
    ensure_required_endings(&mut beats);
    let seed_body = beats
        .iter()
        .find(|beat| beat.kind == "start")
        .map(|beat| beat.body.clone())
        .unwrap_or_default();
    let mut nodes: Vec<Value> = beats
        .iter()
        .map(|beat| compiled_node(game, beat, duration, &seed_body))
        .collect();
    let edges = wire_playable_edges(&beats, &ir.choices, &mut nodes, maximum);
    json!({
        "assets": seeded_assets(&beats),
        "nodes": nodes,
        "edges": edges,
    })
}

fn ensure_required_endings(beats: &mut Vec<ScreenplayBeat>) {
    let has_success = beats.iter().any(|beat| beat.kind == "success");
    let has_failure = beats.iter().any(|beat| beat.kind == "failure");
    let seed = beats
        .iter()
        .find(|beat| beat.kind == "start")
        .or_else(|| beats.first())
        .cloned();
    let Some(seed) = seed else {
        return;
    };
    if !has_success {
        beats.push(ending_from(&seed, "e99", "success", "成功收束"));
    }
    if !has_failure {
        beats.push(ending_from(&seed, "e98", "failure", "失败收束"));
    }
}

fn ending_from(seed: &ScreenplayBeat, id: &str, kind: &str, title: &str) -> ScreenplayBeat {
    ScreenplayBeat {
        id: id.to_owned(),
        kind: kind.to_owned(),
        title: title.to_owned(),
        body: if seed.body.is_empty() {
            format!("{title}。")
        } else {
            seed.body.clone()
        },
        scene: seed.scene.clone(),
        cast: seed.cast.clone(),
    }
}

fn compiled_node(game: &Value, beat: &ScreenplayBeat, duration: i64, seed_body: &str) -> Value {
    let body = if beat.body.trim().is_empty() {
        match beat.kind.as_str() {
            "success" if !seed_body.trim().is_empty() => {
                format!("抉择奏效，局面收束：{seed_body}")
            }
            "failure" if !seed_body.trim().is_empty() => {
                format!("这条路没能挽回局面：{seed_body}")
            }
            _ => format!("{}。", beat.title),
        }
    } else {
        beat.body.clone()
    };
    let scene = if beat.scene.trim().is_empty() {
        format!("{}的现场", beat.title)
    } else {
        beat.scene.clone()
    };
    let cast = if beat.cast.trim().is_empty() {
        "主人公与当场关键人物".to_owned()
    } else {
        beat.cast.clone()
    };
    let prompt = format!(
        "场景：{scene}\n\n角色：{cast}\n\n道具：保持前序节点中关键物件连续。\n\n风格：{}，分辨率：{}。\n光线：服务当前抉择情绪。\n位置：角色、场景和道具的空间关系清晰。\n镜头：一个完整连续镜头呈现：{body}\n前序承接：从前序视频的最后状态无缝继续。\n选择后果：视频结束后停在玩家尚未选择的节点。",
        game["style"].as_str().unwrap_or("真人风格"),
        game["resolution"].as_str().unwrap_or("720p")
    );
    json!({
        "id": beat.id,
        "node_type": beat.kind,
        "title": beat.title,
        "original_text": body,
        "prompt": prompt,
        "reference_asset_ids": beat_material_names(beat),
        "duration_seconds": duration,
        "status": NOT_GENERATED,
        "video_history": [],
    })
}

fn beat_material_names(beat: &ScreenplayBeat) -> Vec<String> {
    let mut names = Vec::new();
    for value in [&beat.scene, &beat.cast] {
        for part in value.split(['、', '，', ',', '/', '\n']) {
            let part = part.trim();
            if part.chars().count() >= 2 {
                names.push(part.to_owned());
            }
        }
    }
    names
}

fn seeded_assets(beats: &[ScreenplayBeat]) -> Vec<Value> {
    let mut assets = Vec::new();
    let mut seen = HashSet::new();
    for beat in beats {
        push_seeded(
            &mut assets,
            &mut seen,
            "scene",
            beat.scene.trim(),
            &beat.body,
        );
        for part in beat.cast.split(['、', '，', ',', '/', '\n']) {
            let name = part.trim();
            if name.chars().count() < 2 {
                continue;
            }
            let kind = if looks_like_prop(name) {
                "prop"
            } else {
                "character"
            };
            push_seeded(&mut assets, &mut seen, kind, name, &beat.body);
        }
    }
    assets
}

fn push_seeded(
    assets: &mut Vec<Value>,
    seen: &mut HashSet<String>,
    kind: &str,
    name: &str,
    body: &str,
) {
    if name.is_empty() || !seen.insert(format!("{kind}:{name}")) {
        return;
    }
    assets.push(json!({
        "id": name,
        "type": kind,
        "name": name,
        "prompt": format!("{name}。{body}"),
    }));
}

fn looks_like_prop(name: &str) -> bool {
    ["机", "器", "灯", "绳", "箱", "钥匙", "录音"]
        .iter()
        .any(|marker| name.contains(marker))
}

fn duration_seconds(game: &Value) -> i64 {
    integer(game, "node_duration_min", 5, 1, 600).clamp(
        *GAME_VIDEO_DURATION_RANGE.start(),
        *GAME_VIDEO_DURATION_RANGE.end(),
    )
}

#[cfg(test)]
#[path = "planner_game_compile_tests.rs"]
mod tests;
