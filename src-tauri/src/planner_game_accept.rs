//! Persist-time acceptance for compiled interactive-game graphs.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use crate::{
    repository::game_validation::GAME_VIDEO_DURATION_RANGE,
    value::{ground_game_video_prompt, NOT_GENERATED},
};

use super::choices::{choice_label_key, fallback_choice_label};
use super::materials::{normalize_assets, resolve_node_references};
use super::{
    clip, expanded_or_source, graph_is_usable, integer, node_text_key, normalize_edge_conditions,
    parse_json_object, place_nodes,
};

/// Accept a graph that a player can finish, even when optional branches were omitted.
pub(crate) fn playable_game_plan(response: &str, game: &Value) -> Option<Value> {
    let parsed = parse_json_object(response)?;
    let node_limit = integer(game, "node_script_max_chars", 400, 1, 1_000_000) as usize;
    let duration_min = duration_seconds(game);
    let duration_max = integer(game, "node_duration_max", 15, duration_min, 600)
        .clamp(duration_min, *GAME_VIDEO_DURATION_RANGE.end());
    let mut ids = HashSet::new();
    let mut kinds = HashMap::new();
    let mut titles = HashMap::new();
    let mut original_texts = HashSet::new();
    let mut video_prompts = HashSet::new();
    let mut nodes = Vec::new();
    for node in parsed["nodes"].as_array()? {
        let id = node["id"].as_str()?.trim();
        let kind = node["node_type"].as_str()?.trim();
        let title = node["title"].as_str()?.trim();
        let original = node["original_text"].as_str()?.trim();
        let prompt = node["prompt"].as_str()?.trim();
        if id.is_empty()
            || title.is_empty()
            || original.is_empty()
            || prompt.is_empty()
            || !["start", "normal", "success", "failure"].contains(&kind)
            || !ids.insert(id.to_owned())
        {
            return None;
        }
        kinds.insert(id.to_owned(), kind.to_owned());
        titles.insert(id.to_owned(), title.to_owned());
        let original = unique_text(clip(original, node_limit), id, &mut original_texts);
        let video_prompt = unique_text(
            ground_game_video_prompt(prompt, &original),
            id,
            &mut video_prompts,
        );
        nodes.push(json!({
            "id": id,
            "node_type": kind,
            "title": clip(title, 80),
            "original_text": original,
            "prompt": video_prompt,
            "reference_asset_ids": node["reference_asset_ids"].as_array().cloned().unwrap_or_default(),
            "duration_seconds": node["duration_seconds"].as_i64().unwrap_or(duration_min).clamp(duration_min, duration_max),
            "status": NOT_GENERATED,
            "video_history": [],
        }));
    }
    if nodes.is_empty() || !usable_ending_counts(&kinds) {
        return None;
    }
    let edges = collect_usable_edges(parsed["edges"].as_array()?, &ids, &titles);
    graph_is_usable(&kinds, &edges, game)?;
    place_nodes(&mut nodes, &edges);
    let assets = normalize_assets(
        parsed["assets"].as_array(),
        &expanded_or_source(game),
        game["style"].as_str().unwrap_or("真人风格"),
    );
    bind_screenplay_references(&mut nodes, &assets);
    let nodes = resolve_node_references(nodes, &assets);
    Some(json!({"assets":assets,"nodes":nodes,"edges":edges}))
}

fn bind_screenplay_references(nodes: &mut [Value], assets: &[Value]) {
    for node in nodes.iter_mut() {
        let haystack = format!(
            "{} {} {}",
            node["title"].as_str().unwrap_or_default(),
            node["original_text"].as_str().unwrap_or_default(),
            node["prompt"].as_str().unwrap_or_default(),
        );
        let mut ids = node["reference_asset_ids"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for asset in assets {
            let name = asset["name"].as_str().unwrap_or_default();
            let id = asset["id"].as_str().unwrap_or(name);
            if name.is_empty() || !haystack.contains(name) {
                continue;
            }
            let already = ids
                .iter()
                .any(|value| value.as_str() == Some(id) || value.as_str() == Some(name));
            if !already {
                ids.push(json!(id));
            }
        }
        node["reference_asset_ids"] = json!(ids);
    }
}

fn collect_usable_edges(
    raw: &[Value],
    ids: &HashSet<String>,
    titles: &HashMap<String, String>,
) -> Vec<Value> {
    let mut edge_ids = HashSet::new();
    let mut choices = HashMap::<String, HashSet<String>>::new();
    let mut sorts = HashMap::<String, i64>::new();
    let mut edges = Vec::new();
    for edge in raw {
        let source = edge["source_node_id"].as_str().unwrap_or_default().trim();
        let target = edge["target_node_id"].as_str().unwrap_or_default().trim();
        if source.is_empty() || target.is_empty() || source == target {
            continue;
        }
        if !ids.contains(source) || !ids.contains(target) {
            continue;
        }
        let option = fallback_choice_label(
            edge["option_text"].as_str().unwrap_or_default(),
            titles.get(target).map(String::as_str).unwrap_or("下一节点"),
        );
        let key = choice_label_key(&option);
        if !choices.entry(source.to_owned()).or_default().insert(key) {
            continue;
        }
        let Some(conditions) = normalize_edge_conditions(edge.get("conditions")) else {
            continue;
        };
        let id = edge["id"].as_str().unwrap_or_default().trim();
        let id = if id.is_empty() || !edge_ids.insert(id.to_owned()) {
            format!("{source}_{target}_{}", edges.len() + 1)
        } else {
            id.to_owned()
        };
        edge_ids.insert(id.clone());
        let sort = sorts.entry(source.to_owned()).or_insert(0);
        *sort += 1;
        edges.push(json!({
            "id": id,
            "source_node_id": source,
            "target_node_id": target,
            "option_text": clip(&option, 80),
            "sort_order": *sort,
            "conditions": conditions,
        }));
    }
    edges
}

fn unique_text(value: String, id: &str, seen: &mut HashSet<String>) -> String {
    let mut text = value;
    let mut suffix = 1;
    while !seen.insert(node_text_key(&text)) {
        text = format!("{text}（{id}-{suffix}）");
        suffix += 1;
    }
    text
}

fn usable_ending_counts(kinds: &HashMap<String, String>) -> bool {
    let count = |kind| {
        kinds
            .values()
            .filter(|value| value.as_str() == kind)
            .count()
    };
    count("start") == 1 && count("success") >= 1 && count("failure") >= 1
}

fn duration_seconds(game: &Value) -> i64 {
    integer(game, "node_duration_min", 5, 1, 600).clamp(
        *GAME_VIDEO_DURATION_RANGE.start(),
        *GAME_VIDEO_DURATION_RANGE.end(),
    )
}
