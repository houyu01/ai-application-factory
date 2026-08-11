//! Material-prompt normalization shared by interactive-game graph planners.

use std::collections::HashMap;

use serde_json::{json, Value};

/// Ensure every basic game material is marked with the visual background context used by manual reference-image configuration.
pub(super) fn asset_prompt(prompt: &str) -> String {
    if prompt.starts_with("叙述背景主题：") {
        prompt.to_owned()
    } else {
        format!("叙述背景主题：互动游戏\n{prompt}")
    }
}

/// Keep model-specified node materials only when their ids or names resolve to the extracted reusable catalog.
pub(super) fn resolve_node_references(mut nodes: Vec<Value>, assets: &[Value]) -> Vec<Value> {
    let mut known = HashMap::new();
    for asset in assets {
        let Some(id) = asset["id"].as_str() else {
            continue;
        };
        known.insert(id.to_owned(), id.to_owned());
        if let Some(name) = asset["name"].as_str() {
            known.insert(name.to_owned(), id.to_owned());
        }
    }
    for node in &mut nodes {
        let mut ids: Vec<String> = Vec::new();
        for reference in node["reference_asset_ids"].as_array().into_iter().flatten() {
            let id = reference
                .as_str()
                .or_else(|| reference["asset_id"].as_str())
                .or_else(|| reference["asset_name"].as_str())
                .and_then(|value| known.get(value));
            if let Some(id) = id {
                if !ids.iter().any(|known| known == id) {
                    ids.push(id.to_owned());
                }
            }
        }
        node["reference_asset_ids"] = json!(ids);
    }
    nodes
}
