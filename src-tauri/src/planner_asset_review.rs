//! Post-decomposition material review shared by drama and interactive-game planners.

use serde_json::{json, Value};

use super::{extracted_assets, AssetEvidence};

/// Reconcile a model material catalog with the completed screenplay before it is persisted.
///
/// Both drama and interactive-game generation call this after their model decomposition has
/// finished. It drops malformed or wrong-category model materials, keeps valid model prompts,
/// and appends every source-grounded missing character, scene, and prop as an image-ready asset.
pub(crate) fn review_assets(script: &str, theme: &str, assets: Vec<Value>) -> Vec<Value> {
    let evidence = AssetEvidence::from_script(script);
    let mut reviewed = Vec::new();
    for asset in assets {
        if let Some(asset) = reviewed_asset(asset, script, &evidence) {
            merge_asset(&mut reviewed, asset);
        }
    }
    for asset in extracted_assets(script, theme) {
        merge_asset(&mut reviewed, asset);
    }
    reconcile_cross_type_conflicts(reviewed, &evidence)
}

/// Remove category collisions and OCR-like name tails after every source-backed asset is merged.
fn reconcile_cross_type_conflicts(mut assets: Vec<Value>, evidence: &AssetEvidence) -> Vec<Value> {
    let prop_names = evidence.names("prop");
    assets.retain(|asset| {
        asset["type"] != "character"
            || !asset["name"].as_str().is_some_and(|name| {
                prop_names.iter().any(|prop| prop == name) || looks_like_object(name)
            })
    });
    let character_names = assets
        .iter()
        .filter(|asset| asset["type"] == "character")
        .filter_map(|asset| asset["name"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assets.retain(|asset| {
        let Some(name) = asset["name"].as_str() else {
            return false;
        };
        asset["type"] != "character"
            || !character_names.iter().any(|short| {
                short != name && name.strip_prefix(short).is_some_and(is_name_boundary_tail)
            })
    });
    assets
}

fn looks_like_object(name: &str) -> bool {
    [
        "公文包",
        "笔记本",
        "文件夹",
        "手提包",
        "钱包",
        "背包",
        "皮箱",
        "日记本",
    ]
    .iter()
    .any(|suffix| name.ends_with(suffix))
}

fn is_name_boundary_tail(tail: &str) -> bool {
    ["一", "一下", "一把", "一个", "一起", "一边", "一眼"]
        .iter()
        .any(|boundary| tail == *boundary)
}

fn reviewed_asset(mut asset: Value, script: &str, evidence: &AssetEvidence) -> Option<Value> {
    let kind = asset_kind(asset["type"].as_str()?)?;
    let supplied_name = asset["name"].as_str()?.trim().to_owned();
    if supplied_name.is_empty() || is_compound_label(&supplied_name) {
        return None;
    }
    let name = match asset["source_evidence"]
        .as_str()
        .filter(|source| !source.trim().is_empty())
    {
        Some(source) => evidence.canonical_name(kind, &supplied_name, source)?,
        None => canonical_name(kind, &supplied_name, script, evidence)?,
    };
    if asset["prompt"]
        .as_str()
        .is_none_or(|prompt| prompt.trim().is_empty())
    {
        return None;
    }
    asset["type"] = json!(kind);
    asset["name"] = json!(name);
    if supplied_name != asset["name"].as_str().unwrap_or_default() {
        asset["aliases"] = json!([supplied_name]);
    }
    Some(asset)
}

fn asset_kind(value: &str) -> Option<&'static str> {
    match value.trim() {
        "character" | "角色" => Some("character"),
        "scene" | "场景" => Some("scene"),
        "prop" | "道具" => Some("prop"),
        _ => None,
    }
}

fn canonical_name(
    kind: &str,
    supplied_name: &str,
    script: &str,
    evidence: &AssetEvidence,
) -> Option<String> {
    let canonical = evidence
        .names(kind)
        .into_iter()
        .filter(|name| supplied_name.contains(name))
        .max_by_key(|name| name.chars().count());
    if canonical.is_some() {
        return canonical;
    }
    if !script.contains(supplied_name) {
        return None;
    }
    let appears_in_another_kind = ["character", "scene", "prop"]
        .into_iter()
        .filter(|other| *other != kind)
        .flat_map(|other| evidence.names(other))
        .any(|name| supplied_name.contains(&name) || name.contains(supplied_name));
    (!appears_in_another_kind).then(|| supplied_name.to_owned())
}

fn is_compound_label(value: &str) -> bool {
    value.contains(['、', '，', ',', '/', '／'])
}

fn merge_asset(assets: &mut Vec<Value>, asset: Value) {
    let Some(existing) = assets
        .iter_mut()
        .find(|current| current["type"] == asset["type"] && current["name"] == asset["name"])
    else {
        assets.push(asset);
        return;
    };
    merge_variants(existing, &asset);
}

fn merge_variants(existing: &mut Value, asset: &Value) {
    let Some(variants) = asset["variants"].as_array() else {
        return;
    };
    let Some(existing_variants) = existing["variants"].as_array_mut() else {
        return;
    };
    for variant in variants {
        let name = variant["name"].as_str().unwrap_or_default();
        if !name.is_empty()
            && !existing_variants
                .iter()
                .any(|current| current["name"] == name)
        {
            existing_variants.push(variant.clone());
        }
    }
}
