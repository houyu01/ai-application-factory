//! Script-grounded asset fallbacks used only when a model response omits reusable materials.

use std::collections::HashSet;

use serde_json::{json, Value};

use super::asset_evidence::AssetEvidence;
use crate::value::NOT_GENERATED;

/// Derive only explicitly mentioned materials so a degraded model response never invents a fixed cast.
pub(crate) fn extracted_assets(script: &str, theme: &str) -> Vec<Value> {
    let mut assets = Vec::new();
    let mut seen = HashSet::new();
    let evidence = AssetEvidence::from_script(script);
    for name in evidence.names("character") {
        add_asset(
            &mut assets,
            &mut seen,
            "character",
            &name,
            &format!("剧本中出现的人物“{name}”，需要保持身份、行为习惯和外观连续性。"),
            theme,
        );
    }
    for name in evidence.names("scene") {
        add_asset(
            &mut assets,
            &mut seen,
            "scene",
            &name,
            &format!("剧本中发生实际剧情的场景“{name}”，需要保持空间布局和光线连续性。"),
            theme,
        );
    }
    for name in evidence.names("prop") {
        add_asset(
            &mut assets,
            &mut seen,
            "prop",
            &name,
            &format!("剧本中出现并参与叙事的道具“{name}”，需要保持材质、形制和使用痕迹连续性。"),
            theme,
        );
    }
    assets
}

fn add_asset(
    assets: &mut Vec<Value>,
    seen: &mut HashSet<String>,
    kind: &str,
    name: &str,
    description: &str,
    theme: &str,
) {
    let name = name.trim();
    if name.is_empty() || !seen.insert(format!("{kind}:{name}")) {
        return;
    }
    assets.push(json!({
        "type": kind,
        "name": name,
        "prompt": format!("叙述背景主题：{theme}\n{description}"),
        "status": NOT_GENERATED,
    }));
}

#[cfg(test)]
mod tests {
    use super::extracted_assets;

    #[test]
    fn fallback_catalog_uses_materials_mentioned_in_the_screenplay() {
        let assets = extracted_assets(
            "场景：青云山演武场·日\n动作：二十岁的林砚穿青布道袍练剑，大师兄赵峰抱着铁刀靠在廊柱上，嘲笑一声撞开他的剑。林砚握紧剑柄。",
            "玄幻",
        );
        let names = assets
            .iter()
            .map(|asset| asset["name"].as_str().expect("name"))
            .collect::<Vec<_>>();
        assert!(names.contains(&"林砚"));
        assert!(names.contains(&"赵峰"));
        assert!(names.contains(&"青云山演武场"));
        assert!(names.contains(&"铁刀"));
        assert!(names.contains(&"剑柄"));
    }
}
