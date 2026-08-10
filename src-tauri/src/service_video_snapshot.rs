//! Immutable rich-prompt snapshots used when video versions are later refined.

use serde_json::{json, Value};

use crate::planner;

/// Freeze image URLs inside rich prompt nodes so version history remains independent of later asset edits.
pub(super) fn prompt_rich(project: &Value, shot: &Value) -> Value {
    let assets = project["assets"].as_array().cloned().unwrap_or_default();
    let mut nodes = shot["prompt_rich"].as_array().cloned().unwrap_or_default();
    for node in &mut nodes {
        if node["type"] != "reference" {
            continue;
        }
        let image = node["image_url"].as_str().map(str::to_owned).or_else(|| {
            planner::resolve_reference_asset(
                &assets,
                node["asset_id"].as_str().unwrap_or_default(),
                node["variant_id"].as_str(),
            )
            .and_then(|asset| asset["image_url"].as_str().map(str::to_owned))
        });
        if let Some(image) = image.filter(|value| !value.is_empty()) {
            node["snapshot_image_url"] = json!(image);
        }
    }
    Value::Array(nodes)
}
