//! Incremental recovery helpers for streamed interactive-game graph plans.

use std::collections::HashSet;

use serde_json::{json, Value};

#[cfg(test)]
use super::parse_json_object;

#[cfg(test)]
const GRAPH_KEYS: [&str; 3] = ["assets", "nodes", "edges"];

/// Retain only completed, independently well-formed records from a streamed graph response.
///
/// A malformed later record must not make the accepted prefix unusable. The worker stores this
/// value in its durable task snapshot and carries it into the next model request.
pub(crate) fn game_graph_progress_checkpoint(response: &str, existing: Option<&Value>) -> Value {
    let mut assets = accepted_items(existing, "assets");
    let mut nodes = accepted_items(existing, "nodes");
    let mut edges = accepted_items(existing, "edges");
    append_accepted(
        &mut assets,
        complete_array_objects(response, "assets"),
        valid_asset,
    );
    append_accepted(
        &mut nodes,
        complete_array_objects(response, "nodes"),
        valid_node,
    );
    append_accepted(
        &mut edges,
        complete_array_objects(response, "edges"),
        valid_edge,
    );
    retain_unique_nodes(&mut nodes);
    retain_unique_edges(&mut edges);
    json!({"assets":assets,"nodes":nodes,"edges":edges})
}

/// Combine the durable prefix with a retry response before whole-graph validation.
///
/// Checkpointed records win for the same id, preventing a retry from replacing video nodes or
/// branches that were already accepted before the failed record.
#[cfg(test)]
pub(crate) fn merge_game_graph_resume(checkpoint: &Value, response: &str) -> Option<Value> {
    let response = parse_json_object(response)?;
    Some(json!({
        "assets": merge_items(checkpoint, &response, "assets"),
        "nodes": merge_items(checkpoint, &response, "nodes"),
        "edges": merge_items(checkpoint, &response, "edges"),
    }))
}

/// Build the model instruction that makes a graph retry continue from saved records.
#[cfg(test)]
pub(crate) fn resume_game_graph_prompt(checkpoint: &Value) -> String {
    if GRAPH_KEYS
        .iter()
        .all(|key| checkpoint[*key].as_array().is_none_or(Vec::is_empty))
    {
        return String::new();
    }
    format!(
        "\n\n【图谱拆分断点】以下素材、视频节点和选择边已通过单项格式校验并已保存。它们是不可修改的既有结果，绝不能重新生成、删除或改写。只返回缺失或曾格式错误的记录；仍使用 {{\"assets\":[...],\"nodes\":[...],\"edges\":[...]}}，无新增记录的数组写 []。若已保存的边引用了缺失节点，只补该节点；若某个来源节点的分支尚未完整，只补缺失的边。返回内容会与这个断点合并后接受完整 DAG 校验。\n\n已保存断点：\n{}",
        checkpoint
    )
}

fn accepted_items(existing: Option<&Value>, key: &str) -> Vec<Value> {
    let Some(items) = existing.and_then(|value| value[key].as_array()) else {
        return Vec::new();
    };
    let valid = match key {
        "assets" => valid_asset,
        "nodes" => valid_node,
        "edges" => valid_edge,
        _ => return Vec::new(),
    };
    let mut result = Vec::new();
    append_accepted(&mut result, items.iter().cloned().collect(), valid);
    result
}

fn append_accepted(target: &mut Vec<Value>, candidates: Vec<Value>, valid: fn(&Value) -> bool) {
    let mut ids = target
        .iter()
        .filter_map(item_id)
        .map(str::to_owned)
        .collect::<HashSet<_>>();
    for candidate in candidates {
        let Some(id) = item_id(&candidate) else {
            continue;
        };
        if valid(&candidate) && ids.insert(id.to_owned()) {
            target.push(candidate);
        }
    }
}

#[cfg(test)]
fn merge_items(checkpoint: &Value, response: &Value, key: &str) -> Vec<Value> {
    let mut result = checkpoint[key].as_array().cloned().unwrap_or_default();
    let mut ids = result
        .iter()
        .filter_map(item_id)
        .map(str::to_owned)
        .collect::<HashSet<_>>();
    for item in response[key].as_array().into_iter().flatten() {
        if item_id(item).is_some_and(|id| ids.insert(id.to_owned())) {
            result.push(item.clone());
        }
    }
    result
}

fn item_id(item: &Value) -> Option<&str> {
    item["id"]
        .as_str()
        .map(str::trim)
        .filter(|id| !id.is_empty())
}

fn valid_asset(item: &Value) -> bool {
    item_id(item).is_some()
        && ["character", "scene", "prop"].contains(&item["type"].as_str().unwrap_or_default())
        && nonempty(item, "name")
        && nonempty(item, "prompt")
}

fn valid_node(item: &Value) -> bool {
    item_id(item).is_some()
        && ["start", "normal", "success", "failure"]
            .contains(&item["node_type"].as_str().unwrap_or_default())
        && nonempty(item, "title")
        && nonempty(item, "original_text")
        && nonempty(item, "prompt")
        && item.get("reference_asset_ids").is_none_or(|value| {
            value
                .as_array()
                .is_some_and(|ids| ids.iter().all(Value::is_string))
        })
}

fn valid_edge(item: &Value) -> bool {
    let source = item["source_node_id"]
        .as_str()
        .map(str::trim)
        .unwrap_or_default();
    let target = item["target_node_id"]
        .as_str()
        .map(str::trim)
        .unwrap_or_default();
    item_id(item).is_some()
        && !source.is_empty()
        && source != target
        && !target.is_empty()
        && nonempty(item, "option_text")
        && valid_edge_conditions(item.get("conditions"))
}

fn retain_unique_nodes(nodes: &mut Vec<Value>) {
    let mut originals = HashSet::new();
    let mut prompts = HashSet::new();
    nodes.retain(|node| {
        let original = text_key(node["original_text"].as_str().unwrap_or_default());
        let prompt = text_key(node["prompt"].as_str().unwrap_or_default());
        originals.insert(original) && prompts.insert(prompt)
    });
}

fn retain_unique_edges(edges: &mut Vec<Value>) {
    let mut choices = HashSet::new();
    edges.retain(|edge| {
        let source = edge["source_node_id"].as_str().unwrap_or_default().trim();
        let option = text_key(edge["option_text"].as_str().unwrap_or_default());
        choices.insert(format!("{source}\u{0}{option}"))
    });
}

fn valid_edge_conditions(value: Option<&Value>) -> bool {
    let Some(value) = value else {
        return true;
    };
    let Some(values) = value.as_object() else {
        return false;
    };
    values.keys().all(|key| key == "requires" || key == "set")
        && values.values().all(|entries| {
            entries.as_object().is_some_and(|entries| {
                entries.iter().all(|(key, value)| {
                    !key.is_empty()
                        && key.len() <= 64
                        && key.chars().all(|character| {
                            character.is_ascii_lowercase()
                                || character.is_ascii_digit()
                                || character == '_'
                        })
                        && (value.is_string() || value.is_number() || value.is_boolean())
                })
            })
        })
}

fn text_key(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn nonempty(item: &Value, key: &str) -> bool {
    item[key]
        .as_str()
        .is_some_and(|value| !value.trim().is_empty())
}

fn complete_array_objects(response: &str, key: &str) -> Vec<Value> {
    let Some(mut cursor) = root_array_start(response, key) else {
        return Vec::new();
    };
    let bytes = response.as_bytes();
    let mut records = Vec::new();
    while cursor < bytes.len() {
        match bytes[cursor] {
            b' ' | b'\n' | b'\r' | b'\t' | b',' => cursor += 1,
            b']' => break,
            b'{' => {
                let Some(end) = complete_object_end(bytes, cursor) else {
                    break;
                };
                if let Ok(value) = serde_json::from_str(&response[cursor..end]) {
                    records.push(value);
                }
                cursor = end;
            }
            _ => cursor += 1,
        }
    }
    records
}

fn root_array_start(response: &str, key: &str) -> Option<usize> {
    let bytes = response.as_bytes();
    let mut cursor = bytes.iter().position(|byte| *byte == b'{')? + 1;
    let mut depth = 1_usize;
    while cursor < bytes.len() && depth > 0 {
        match bytes[cursor] {
            b'"' => {
                let end = json_string_end(bytes, cursor)?;
                if depth == 1 && response.get(cursor + 1..end - 1) == Some(key) {
                    let mut value = end;
                    while bytes.get(value).is_some_and(u8::is_ascii_whitespace) {
                        value += 1;
                    }
                    if bytes.get(value) == Some(&b':') {
                        value += 1;
                        while bytes.get(value).is_some_and(u8::is_ascii_whitespace) {
                            value += 1;
                        }
                        if bytes.get(value) == Some(&b'[') {
                            return Some(value + 1);
                        }
                    }
                }
                cursor = end;
            }
            b'{' => {
                depth += 1;
                cursor += 1;
            }
            b'}' => {
                depth -= 1;
                cursor += 1;
            }
            _ => cursor += 1,
        }
    }
    None
}

fn complete_object_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start;
    let mut depth = 0_usize;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' => cursor = json_string_end(bytes, cursor)?,
            b'{' => {
                depth += 1;
                cursor += 1;
            }
            b'}' => {
                depth -= 1;
                cursor += 1;
                if depth == 0 {
                    return Some(cursor);
                }
            }
            _ => cursor += 1,
        }
    }
    None
}

fn json_string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start + 1;
    let mut escaped = false;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' if !escaped => escaped = true,
            b'"' if !escaped => return Some(cursor + 1),
            _ => escaped = false,
        }
        cursor += 1;
    }
    None
}
