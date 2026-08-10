//! Reference-node helpers that resolve a character's base or alternative visual form.

use serde_json::{json, Value};

/// Find the exact visual material requested by one rich-prompt reference node.
///
/// Storyboard persistence calls this through `planner` before image generation, while
/// video preparation calls the same `asset_id`/`variant_id` contract after reload.
pub(crate) fn resolve_asset(
    assets: &[Value],
    asset_id: &str,
    variant_id: Option<&str>,
) -> Option<Value> {
    let asset = assets
        .iter()
        .find(|asset| asset["id"].as_str() == Some(asset_id))?;
    let Some(variant_id) = variant_id.filter(|id| !id.is_empty()) else {
        return Some(asset.clone());
    };
    let variant = asset["variants"]
        .as_array()?
        .iter()
        .find(|variant| variant["id"].as_str() == Some(variant_id))?;
    Some(json!({
        "id": asset_id,
        "type": asset["type"],
        "name": format!("{} · {}", asset["name"].as_str().unwrap_or("角色"), variant["name"].as_str().unwrap_or("其他形态")),
        "prompt": variant["prompt"],
        "image_url": variant["image_url"],
        "status": variant["status"],
        "variant_id": variant_id,
        "parent_asset": asset,
    }))
}

/// Form a durable key so a base character and each alternative form receive distinct @图 numbers.
pub(crate) fn key(asset_id: &str, variant_id: Option<&str>) -> String {
    match variant_id.filter(|id| !id.is_empty()) {
        Some(variant) => format!("{asset_id}:{variant}"),
        None => asset_id.to_owned(),
    }
}

/// Convert decomposition-time name/form hints into persisted rich-reference nodes.
pub(crate) fn planned_nodes(assets: &[Value], requests: &[Value]) -> Vec<Value> {
    let mut nodes = Vec::new();
    for request in requests {
        let kind = request["asset_type"]
            .as_str()
            .or_else(|| request["type"].as_str())
            .unwrap_or_default();
        let name = request["asset_name"]
            .as_str()
            .or_else(|| request["name"].as_str())
            .unwrap_or_default();
        let Some(asset) = assets.iter().find(|asset| {
            asset["type"].as_str() == Some(kind) && asset["name"].as_str() == Some(name)
        }) else {
            continue;
        };
        let variant_id = request["variant_name"]
            .as_str()
            .or_else(|| request["form_name"].as_str())
            .and_then(|form| {
                asset["variants"]
                    .as_array()?
                    .iter()
                    .find(|variant| variant["name"].as_str() == Some(form))?["id"]
                    .as_str()
            });
        let resolved = resolve_asset(assets, asset["id"].as_str().unwrap_or_default(), variant_id)
            .unwrap_or_else(|| asset.clone());
        nodes.push(json!({
            "type": "reference",
            "asset_id": asset["id"],
            "variant_id": variant_id,
            "asset_type": asset["type"],
            "label": resolved["name"],
            "image_url": resolved["image_url"],
        }));
    }
    nodes
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{planned_nodes, resolve_asset};

    #[test]
    fn growth_story_uses_the_child_reference_instead_of_the_adult_base_image() {
        let assets = vec![json!({
            "id":"lin-yan","type":"character","name":"林砚","image_url":"adult.png",
            "variants":[{"id":"child","name":"幼年形态","image_url":"child.png"}]
        })];
        let nodes = planned_nodes(
            &assets,
            &[json!({"asset_type":"character","asset_name":"林砚","variant_name":"幼年形态"})],
        );
        let form =
            resolve_asset(&assets, "lin-yan", nodes[0]["variant_id"].as_str()).expect("child form");
        assert_eq!(nodes[0]["label"], "林砚 · 幼年形态");
        assert_eq!(form["image_url"], "child.png");
    }
}
