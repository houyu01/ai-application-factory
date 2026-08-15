//! Reviewable, non-regular DAG fallback used when a language model cannot return a game graph.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use crate::value::{ground_game_video_prompt, NOT_GENERATED};

use crate::repository::game_validation::GAME_VIDEO_DURATION_RANGE;

use super::super::extracted_assets;
use super::{clip, expanded_or_source, integer, place_nodes};

fn fallback_assets(script: &str) -> Vec<Value> {
    let assets = extracted_assets(script, "互动游戏");
    if assets.is_empty() {
        vec![
            json!({"id":"character_001","type":"character","name":"主角","prompt":"叙述背景主题：互动游戏\n互动游戏主角，稳定外貌、服装和情绪特征。","status":NOT_GENERATED}),
        ]
    } else {
        assets
    }
}

/// Build a playable DAG with an immediate failure path and a converging investigation route.
pub(crate) fn fallback_game_plan(game: &Value) -> Value {
    let script = expanded_or_source(game);
    let maximum = integer(
        game,
        "branch_max",
        4,
        integer(game, "branch_min", 2, 2, 4),
        4,
    ) as usize;
    let successes = integer(game, "success_ending_count", 2, 1, 100) as usize;
    let failures = integer(game, "failure_ending_count", 12, 1, 200) as usize;
    let ending_count = successes + failures;
    let text_limit = integer(game, "node_script_max_chars", 400, 1, 1_000_000) as usize;
    let duration = ((integer(game, "node_duration_min", 5, 1, 600)
        + integer(game, "node_duration_max", 15, 1, 600))
        / 2)
    .clamp(
        *GAME_VIDEO_DURATION_RANGE.start(),
        *GAME_VIDEO_DURATION_RANGE.end(),
    );
    let mut nodes = vec![fallback_node(
        game,
        "start",
        "start",
        "起始视频",
        &clip(&script, text_limit),
        duration,
    )];
    let terminals = add_terminals(
        game, &mut nodes, successes, failures, &script, text_limit, duration,
    );
    let direct_failure = terminals[successes].clone();
    let mut choices = Vec::<(String, Option<String>)>::new();
    if maximum == 2 {
        add_binary_opening(
            game,
            &mut nodes,
            &mut choices,
            &direct_failure,
            &script,
            text_limit,
            duration,
        );
        let extra = ending_count.saturating_sub(5);
        add_terminal_chain(
            game,
            &mut nodes,
            &mut choices,
            "merge",
            extra,
            maximum,
            &script,
            text_limit,
            duration,
        );
    } else {
        add_wide_opening(
            game,
            &mut nodes,
            &mut choices,
            &direct_failure,
            maximum,
            &script,
            text_limit,
            duration,
        );
        let base_slots = maximum * maximum - maximum + 2;
        let extra = ending_count
            .saturating_sub(base_slots)
            .div_ceil(maximum - 1);
        add_terminal_chain(
            game,
            &mut nodes,
            &mut choices,
            "merge",
            extra,
            maximum,
            &script,
            text_limit,
            duration,
        );
    }
    let state_gate = if choices.iter().any(|(source, _)| source == "decision_4") {
        "decision_4"
    } else {
        "merge"
    };
    fill_terminal_choices(
        &mut choices,
        &terminals,
        &direct_failure,
        successes,
        state_gate,
    );
    let edges = materialize_edges(choices, &terminals, &direct_failure, state_gate);
    place_nodes(&mut nodes, &edges);
    json!({"assets":fallback_assets(&script),"nodes":nodes,"edges":edges})
}

fn add_terminals(
    game: &Value,
    nodes: &mut Vec<Value>,
    successes: usize,
    failures: usize,
    script: &str,
    text_limit: usize,
    duration: i64,
) -> Vec<String> {
    (0..successes + failures)
        .map(|index| {
            let success = index < successes;
            let ordinal = if success {
                index + 1
            } else {
                index - successes + 1
            };
            let id = format!("ending_{}", index + 1);
            nodes.push(fallback_node(
                game,
                &id,
                if success { "success" } else { "failure" },
                &format!("{}结局 {ordinal}", if success { "成功" } else { "失败" }),
                &clip_end(script, text_limit),
                duration,
            ));
            id
        })
        .collect()
}

fn add_binary_opening(
    game: &Value,
    nodes: &mut Vec<Value>,
    choices: &mut Vec<(String, Option<String>)>,
    direct_failure: &str,
    script: &str,
    text_limit: usize,
    duration: i64,
) {
    for (id, title) in [
        ("fork", "继续追查"),
        ("route_left", "左路调查"),
        ("route_right", "右路调查"),
        ("merge", "线索汇合"),
    ] {
        nodes.push(fallback_node(
            game,
            id,
            "normal",
            title,
            &clip(script, text_limit),
            duration,
        ));
    }
    add_choices(
        choices,
        "start",
        vec![Some(direct_failure.to_owned()), Some("fork".to_owned())],
    );
    add_choices(
        choices,
        "fork",
        vec![
            Some("route_left".to_owned()),
            Some("route_right".to_owned()),
        ],
    );
    for route in ["route_left", "route_right"] {
        add_choices(choices, route, vec![Some("merge".to_owned()), None]);
    }
}

fn add_wide_opening(
    game: &Value,
    nodes: &mut Vec<Value>,
    choices: &mut Vec<(String, Option<String>)>,
    direct_failure: &str,
    maximum: usize,
    script: &str,
    text_limit: usize,
    duration: i64,
) {
    nodes.push(fallback_node(
        game,
        "merge",
        "normal",
        "线索汇合",
        &clip(script, text_limit),
        duration,
    ));
    let routes = (1..maximum)
        .map(|index| format!("route_{index}"))
        .collect::<Vec<_>>();
    let mut start = vec![Some(direct_failure.to_owned())];
    start.extend(routes.iter().cloned().map(Some));
    add_choices(choices, "start", start);
    for (index, route) in routes.iter().enumerate() {
        nodes.push(fallback_node(
            game,
            route,
            "normal",
            &format!("调查路径 {}", index + 1),
            &clip(script, text_limit),
            duration,
        ));
        let mut next = vec![Some("merge".to_owned())];
        next.extend(vec![None; maximum - 1]);
        add_choices(choices, route, next);
    }
}

fn add_terminal_chain(
    game: &Value,
    nodes: &mut Vec<Value>,
    choices: &mut Vec<(String, Option<String>)>,
    initial: &str,
    extra: usize,
    maximum: usize,
    script: &str,
    text_limit: usize,
    duration: i64,
) {
    let mut current = initial.to_owned();
    for index in 0..extra {
        let next = format!("decision_{}", index + 1);
        let mut outgoing = vec![Some(next.clone())];
        outgoing.extend(vec![None; maximum - 1]);
        add_choices(choices, &current, outgoing);
        nodes.push(fallback_node(
            game,
            &next,
            "normal",
            &format!("进一步抉择 {}", index + 1),
            &clip(script, text_limit),
            duration,
        ));
        current = next;
    }
    add_choices(choices, &current, vec![None; maximum]);
}

fn add_choices(
    choices: &mut Vec<(String, Option<String>)>,
    source: &str,
    targets: Vec<Option<String>>,
) {
    choices.extend(
        targets
            .into_iter()
            .map(|target| (source.to_owned(), target)),
    );
}

fn fill_terminal_choices(
    choices: &mut [(String, Option<String>)],
    terminals: &[String],
    direct_failure: &str,
    successes: usize,
    state_gate: &str,
) {
    let preferred = [
        terminals[0].clone(),
        terminals
            .get(successes + 1)
            .cloned()
            .unwrap_or_else(|| direct_failure.to_owned()),
    ];
    for (index, (_, target)) in choices
        .iter_mut()
        .filter(|(source, target)| source == state_gate && target.is_none())
        .take(preferred.len())
        .enumerate()
    {
        *target = Some(preferred[index].clone());
    }
    let assigned = choices
        .iter()
        .filter_map(|(_, target)| target.as_ref())
        .filter(|target| terminals.contains(target))
        .collect::<HashSet<_>>();
    let remaining = terminals
        .iter()
        .filter(|id| !assigned.contains(id))
        .cloned()
        .collect::<Vec<_>>();
    let mut index = 0;
    for (_, target) in choices.iter_mut().filter(|(_, target)| target.is_none()) {
        let selected = if index < remaining.len() {
            remaining[index].clone()
        } else {
            terminals[(index - remaining.len()) % terminals.len()].clone()
        };
        *target = Some(selected);
        index += 1;
    }
}

fn materialize_edges(
    choices: Vec<(String, Option<String>)>,
    terminals: &[String],
    direct_failure: &str,
    state_gate: &str,
) -> Vec<Value> {
    let mut orders = HashMap::<String, i64>::new();
    choices.into_iter().map(|(source, target)| {
        let target = target.expect("fallback choices are complete");
        let order = orders.entry(source.clone()).and_modify(|value| *value += 1).or_insert(1);
        let option_text = if source == "start" && target == direct_failure { "贸然行动，立即触发失败".to_owned() } else if terminals.contains(&target) { format!("承担当前选择的后果 {order}") } else if target == "merge" { "带着线索前往汇合点".to_owned() } else { format!("继续追查关键线索 {order}") };
        let conditions = fallback_conditions(&source, &target, terminals, direct_failure, state_gate);
        json!({"id":format!("{source}_option_{order}"),"source_node_id":source,"target_node_id":target,"option_text":option_text,"sort_order":order,"conditions":conditions})
    }).collect()
}

fn fallback_conditions(
    source: &str,
    target: &str,
    terminals: &[String],
    direct_failure: &str,
    state_gate: &str,
) -> Value {
    let successes = terminals
        .iter()
        .position(|terminal| terminal == direct_failure)
        .unwrap_or(1);
    let delayed_failure = terminals
        .get(successes + 1)
        .map_or(direct_failure, String::as_str);
    match (source, target) {
        ("fork", "route_left") | ("route_1", "merge") => {
            json!({"set":{"evidence_secured":true}})
        }
        ("fork", "route_right") | ("route_2", "merge") => {
            json!({"set":{"evidence_secured":false}})
        }
        (source, target) if source == state_gate && target == terminals[0] => {
            json!({"requires":{"evidence_secured":true}})
        }
        (source, target) if source == state_gate && target == delayed_failure => {
            json!({"requires":{"evidence_secured":false}})
        }
        _ => json!({}),
    }
}

fn fallback_node(
    game: &Value,
    id: &str,
    kind: &str,
    title: &str,
    text: &str,
    duration: i64,
) -> Value {
    let text = format!("【{title}】{text}");
    let prompt = format!("场景：@图1（待选择场景），根据互动剧情保持连续场景。\n\n角色：@图2（待选择角色），围绕“{title}”推进。\n\n道具：@图3（待选择道具），保持前序节点中的数量、材质和位置连续。\n\n风格：{}，分辨率：{}。\n光线：根据当前抉择的情绪延续。\n位置：角色、场景和道具的空间关系清晰。\n镜头：一个完整连续镜头呈现行动与后果。\n前序承接：从前序视频的最后状态无缝继续。\n选择后果：视频结束后为下一节点提供明确分支。",game["style"].as_str().unwrap_or("真人风格"),game["resolution"].as_str().unwrap_or("720p"));
    let prompt = ground_game_video_prompt(&prompt, &text);
    json!({"id":id,"node_type":kind,"title":title,"original_text":text,"prompt":prompt,"reference_asset_ids":[],"duration_seconds":duration,"status":NOT_GENERATED,"video_history":[]})
}

fn clip_end(value: &str, limit: usize) -> String {
    value
        .chars()
        .rev()
        .take(limit)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}
