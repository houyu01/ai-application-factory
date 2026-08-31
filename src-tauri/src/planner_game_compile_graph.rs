//! Keep compiled interactive-game edges playable: drop cycles, repair dead ends, attach required endings.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Map, Value};

use super::choices::fallback_choice_label;
use super::clip;
use super::screenplay::{ScreenplayBeat, ScreenplayChoice};

pub(super) fn wire_playable_edges(
    beats: &[ScreenplayBeat],
    choices: &[ScreenplayChoice],
    nodes: &mut Vec<Value>,
    maximum: usize,
) -> Vec<Value> {
    let ids: HashSet<String> = beats.iter().map(|beat| beat.id.clone()).collect();
    let titles: HashMap<String, String> = beats
        .iter()
        .map(|beat| (beat.id.clone(), beat.title.clone()))
        .collect();
    let kinds: HashMap<String, String> = beats
        .iter()
        .map(|beat| (beat.id.clone(), beat.kind.clone()))
        .collect();
    let mut edges = Vec::new();
    for choice in choices {
        try_add_edge(&mut edges, choice, &ids, &kinds, &titles, maximum);
    }
    repair_dead_ends(&mut edges, beats, &kinds, &titles, maximum);
    attach_required_endings(&mut edges, beats, &kinds, &titles, maximum);
    drop_unreachable(nodes, &mut edges, &kinds);
    edges
}

fn try_add_edge(
    edges: &mut Vec<Value>,
    choice: &ScreenplayChoice,
    ids: &HashSet<String>,
    kinds: &HashMap<String, String>,
    titles: &HashMap<String, String>,
    maximum: usize,
) {
    if !ids.contains(&choice.source) || !ids.contains(&choice.target) {
        return;
    }
    if kinds
        .get(&choice.source)
        .is_some_and(|kind| kind == "success" || kind == "failure")
    {
        return;
    }
    if outgoing(edges, &choice.source) >= maximum {
        return;
    }
    if would_cycle(edges, &choice.source, &choice.target) {
        return;
    }
    push_edge(
        edges,
        &choice.source,
        &choice.target,
        fallback_choice_label(
            &choice.option,
            titles
                .get(&choice.target)
                .map(String::as_str)
                .unwrap_or("下一节点"),
        ),
        conditions_from(choice),
    );
}

fn repair_dead_ends(
    edges: &mut Vec<Value>,
    beats: &[ScreenplayBeat],
    kinds: &HashMap<String, String>,
    titles: &HashMap<String, String>,
    maximum: usize,
) {
    let endings: Vec<&ScreenplayBeat> = beats
        .iter()
        .filter(|beat| beat.kind == "success" || beat.kind == "failure")
        .collect();
    let mut used_endings = HashSet::new();
    for edge in edges.iter() {
        if let Some(target) = edge["target_node_id"].as_str() {
            if kinds
                .get(target)
                .is_some_and(|kind| kind == "success" || kind == "failure")
            {
                used_endings.insert(target.to_owned());
            }
        }
    }
    let internals: Vec<String> = beats
        .iter()
        .filter(|beat| beat.kind == "start" || beat.kind == "normal")
        .map(|beat| beat.id.clone())
        .collect();
    for id in internals {
        while outgoing(edges, &id) == 0 {
            let target = next_repair_target(&id, beats, &endings, &used_endings);
            let Some(target) = target else {
                break;
            };
            if would_cycle(edges, &id, &target) || outgoing(edges, &id) >= maximum {
                break;
            }
            if kinds
                .get(&target)
                .is_some_and(|kind| kind == "success" || kind == "failure")
            {
                used_endings.insert(target.clone());
            }
            let title = titles
                .get(&target)
                .map(String::as_str)
                .unwrap_or("下一节点");
            push_edge(
                edges,
                &id,
                &target,
                fallback_choice_label("", title),
                json!({}),
            );
        }
    }
}

fn attach_required_endings(
    edges: &mut Vec<Value>,
    beats: &[ScreenplayBeat],
    kinds: &HashMap<String, String>,
    titles: &HashMap<String, String>,
    maximum: usize,
) {
    let internals: Vec<String> = beats
        .iter()
        .filter(|beat| beat.kind == "start" || beat.kind == "normal")
        .map(|beat| beat.id.clone())
        .collect();
    for kind in ["success", "failure"] {
        if beats
            .iter()
            .any(|beat| beat.kind == kind && incoming(edges, &beat.id) > 0)
        {
            continue;
        }
        let Some(target) = beats
            .iter()
            .find(|beat| beat.kind == kind)
            .map(|beat| beat.id.clone())
        else {
            continue;
        };
        try_attach(edges, &internals, &target, kinds, titles, maximum);
    }
}

fn try_attach(
    edges: &mut Vec<Value>,
    internals: &[String],
    target: &str,
    kinds: &HashMap<String, String>,
    titles: &HashMap<String, String>,
    maximum: usize,
) {
    let title = titles.get(target).map(String::as_str).unwrap_or("下一节点");
    let mut sources = internals.to_vec();
    sources.sort_by_key(|id| {
        (
            usize::from(kinds.get(id).is_some_and(|kind| kind != "start")),
            outgoing(edges, id),
        )
    });
    if let Some(source) = sources
        .iter()
        .find(|id| outgoing(edges, id) < maximum && !would_cycle(edges, id, target))
    {
        push_edge(
            edges,
            source,
            target,
            fallback_choice_label("", title),
            json!({}),
        );
        return;
    }
    for source in &sources {
        let extra = edges.iter().position(|edge| {
            edge["source_node_id"] == *source
                && incoming(edges, edge["target_node_id"].as_str().unwrap_or_default()) > 1
        });
        let Some(index) = extra else {
            continue;
        };
        let removed = edges.remove(index);
        if would_cycle(edges, source, target) {
            edges.insert(index, removed);
            continue;
        }
        push_edge(
            edges,
            source,
            target,
            fallback_choice_label("", title),
            json!({}),
        );
        return;
    }
}

fn incoming(edges: &[Value], id: &str) -> usize {
    edges
        .iter()
        .filter(|edge| edge["target_node_id"] == id)
        .count()
}

fn next_repair_target(
    source: &str,
    beats: &[ScreenplayBeat],
    endings: &[&ScreenplayBeat],
    used_endings: &HashSet<String>,
) -> Option<String> {
    let unused = endings
        .iter()
        .find(|beat| !used_endings.contains(&beat.id))
        .map(|beat| beat.id.clone());
    if unused.is_some() {
        return unused;
    }
    let index = beats.iter().position(|beat| beat.id == source)?;
    beats
        .iter()
        .skip(index + 1)
        .find(|beat| beat.id != source)
        .map(|beat| beat.id.clone())
        .or_else(|| endings.first().map(|beat| beat.id.clone()))
}

fn drop_unreachable(
    nodes: &mut Vec<Value>,
    edges: &mut Vec<Value>,
    kinds: &HashMap<String, String>,
) {
    let start = kinds
        .iter()
        .find_map(|(id, kind)| (kind == "start").then(|| id.clone()));
    let Some(start) = start else {
        return;
    };
    let mut outgoing_links = HashMap::<String, Vec<String>>::new();
    for edge in edges.iter() {
        if let (Some(source), Some(target)) = (
            edge["source_node_id"].as_str(),
            edge["target_node_id"].as_str(),
        ) {
            outgoing_links
                .entry(source.to_owned())
                .or_default()
                .push(target.to_owned());
        }
    }
    let mut seen = HashSet::new();
    let mut stack = vec![start];
    while let Some(id) = stack.pop() {
        if seen.insert(id.clone()) {
            stack.extend(outgoing_links.get(&id).into_iter().flatten().cloned());
        }
    }
    nodes.retain(|node| {
        node["id"]
            .as_str()
            .is_some_and(|id| seen.contains(id) || node["node_type"] == "start")
    });
    let kept: HashSet<String> = nodes
        .iter()
        .filter_map(|node| node["id"].as_str().map(str::to_owned))
        .collect();
    edges.retain(|edge| {
        edge["source_node_id"]
            .as_str()
            .is_some_and(|id| kept.contains(id))
            && edge["target_node_id"]
                .as_str()
                .is_some_and(|id| kept.contains(id))
    });
}

fn push_edge(
    edges: &mut Vec<Value>,
    source: &str,
    target: &str,
    option: String,
    conditions: Value,
) {
    let order = outgoing(edges, source) as i64 + 1;
    edges.push(json!({
        "id": format!("{source}_{target}_{order}"),
        "source_node_id": source,
        "target_node_id": target,
        "option_text": clip(&option, 80),
        "sort_order": order,
        "conditions": conditions,
    }));
}

fn conditions_from(choice: &ScreenplayChoice) -> Value {
    let mut conditions = Map::new();
    if let Some((key, value)) = &choice.requires {
        conditions.insert("requires".to_owned(), json!({key: value}));
    }
    if let Some((key, value)) = &choice.set {
        conditions.insert("set".to_owned(), json!({key: value}));
    }
    Value::Object(conditions)
}

fn outgoing(edges: &[Value], source: &str) -> usize {
    edges
        .iter()
        .filter(|edge| edge["source_node_id"] == source)
        .count()
}

fn would_cycle(edges: &[Value], source: &str, target: &str) -> bool {
    if source == target {
        return true;
    }
    let mut next_nodes: HashMap<String, Vec<String>> = HashMap::new();
    for edge in edges {
        if let (Some(from), Some(to)) = (
            edge["source_node_id"].as_str(),
            edge["target_node_id"].as_str(),
        ) {
            next_nodes
                .entry(from.to_owned())
                .or_default()
                .push(to.to_owned());
        }
    }
    let mut pending = vec![target.to_owned()];
    let mut seen = HashSet::new();
    while let Some(id) = pending.pop() {
        if id == source {
            return true;
        }
        if seen.insert(id.clone()) {
            pending.extend(next_nodes.get(&id).into_iter().flatten().cloned());
        }
    }
    false
}
