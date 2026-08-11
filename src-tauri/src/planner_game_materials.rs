//! Material-prompt normalization shared by interactive-game graph planners.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::value::NOT_GENERATED;

use super::review_assets;

/// Exact image-prompt sections required from the game graph model for every reusable material.
pub(super) const GAME_ASSET_PROMPT_CONTRACT: &str = "素材图片提示词格式（最高优先级）：每项 asset.prompt 都必须使用短剧素材的分段格式。第一行固定为“叙述背景主题：互动游戏”，第二行固定为“风格：当前项目风格”。角色依次写“角色身份与性格：”“外观设定：”“连续性要求：”；场景依次写“场景名称与剧情用途：”“空间与主体：”“陈设与氛围：”；道具依次写“道具名称与叙事用途：”“外观细节：”“呈现限制：”。每段都必须针对该素材在剧本中的具体作用填写；角色写身份、性格/行为、年龄形态、脸型眉眼、发型、体态、服装和配饰，并说明跨镜头一致性；场景写剧情用途、空间纵深/动线、陈设、色调光线，且无人物、无背景文字、无水印；道具写叙事用途、颜色、材质、尺寸/形制、纹理/磨损和表面文字限制，且单一主体、无品牌、无多余文字、无水印。\n\n主人公素材规则（不可省略）：先读取“【互动剧本总览】”中的主人公；若原稿未明确而剧本里有多名角色，依据谁承担玩家目标、主要行动和结局后果判定唯一主人公；若仍无法判定，补全一名有真实姓名的主人公并让其进入起始节点。assets 必须包含该主人公的独立 character，name 必须是剧本持续使用的真实姓名，角色身份与性格段必须明确“玩家扮演的主人公”及其目标；不得以“玩家”“主角”或群像替代角色素材，不能让游戏只有场景、道具或配角而没有主人公。";

/// Review graph-model materials against the completed branch screenplay before node references are resolved.
///
/// The interactive-game graph planner invokes this after validating graph topology. It retains only
/// syntactically valid model materials, then lets the shared reviewer reject wrong categories and
/// synthesize any source-grounded assets the model missed. Every retained material is then written
/// in the same image-prompt structure used by short-drama characters, scenes, and props.
pub(super) fn normalize_assets(raw: Option<&Vec<Value>>, script: &str, style: &str) -> Vec<Value> {
    let assets = raw
        .into_iter()
        .flatten()
        .filter_map(|asset| {
            let kind = asset["type"].as_str()?.trim();
            let name = asset["name"].as_str()?.trim();
            let prompt = asset["prompt"].as_str()?.trim();
            (["character", "scene", "prop", "角色", "场景", "道具"].contains(&kind)
                && !name.is_empty()
                && !prompt.is_empty())
            .then(|| {
                let mut asset = asset.clone();
                asset["id"] = json!(asset["id"].as_str().unwrap_or(name));
                asset["prompt"] = json!(prompt.to_owned());
                asset["status"] = json!(NOT_GENERATED);
                asset
            })
        })
        .collect();
    let mut assets = review_assets(script, "互动游戏", assets);
    for (index, asset) in assets.iter_mut().enumerate() {
        if asset["id"].as_str().is_none_or(str::is_empty) {
            asset["id"] = json!(format!(
                "{}:{}",
                asset["type"].as_str().unwrap_or("prop"),
                asset["name"].as_str().unwrap_or("material"),
            ));
        }
        let kind = asset["type"].as_str().unwrap_or("prop");
        let name = asset["name"].as_str().unwrap_or("未命名素材");
        let source = asset["prompt"].as_str().unwrap_or_default();
        asset["prompt"] = json!(short_drama_asset_prompt(kind, name, source, style, index));
    }
    assets
}

/// Make game material images use the same editable section layout as short-drama materials.
fn short_drama_asset_prompt(
    kind: &str,
    name: &str,
    source: &str,
    style: &str,
    index: usize,
) -> String {
    let source = prompt_body(source);
    match kind {
        "character" => format!(
            "叙述背景主题：互动游戏\n风格：{style}\n角色身份与性格：{name}。{source}\n外观设定：{}\n连续性要求：发型、脸部特征、身型、服装层次和随身配饰在后续镜头中保持一致；如剧本明确其他年龄、状态或换装形态，必须使用对应形态参考图；呈现{style}视觉细节，无文字水印。",
            character_appearance(index),
        ),
        "scene" => format!(
            "叙述背景主题：互动游戏\n风格：{style}\n场景名称与剧情用途：{name}，{source}\n空间与主体：{}\n陈设与氛围：场内物件带有真实使用状态，色调、主光和空气感服务于剧情；无人物、无背景文字、无水印。",
            scene_structure(index),
        ),
        _ => format!(
            "叙述背景主题：互动游戏\n风格：{style}\n道具名称与叙事用途：{name}，{source}\n外观细节：{}\n呈现限制：单一主体清晰完整，干净静物构图，材质纹理和磨损可辨，无品牌、无多余文字、无水印。",
            prop_details(index),
        ),
    }
}

fn prompt_body(value: &str) -> String {
    let content = value
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if ["叙述背景主题：", "风格："]
                .iter()
                .any(|prefix| line.starts_with(prefix))
            {
                return None;
            }
            let text = [
                "角色身份与性格：",
                "外观设定：",
                "连续性要求：",
                "场景名称与剧情用途：",
                "空间与主体：",
                "陈设与氛围：",
                "道具名称与叙事用途：",
                "外观细节：",
                "呈现限制：",
            ]
            .iter()
            .find_map(|prefix| line.strip_prefix(prefix).map(str::trim))
            .unwrap_or(line);
            (!text.is_empty()).then_some(text)
        })
        .collect::<Vec<_>>()
        .join("；");
    if content.is_empty() {
        "剧本中实际出现、可复用的关键视觉素材。".to_owned()
    } else {
        content
    }
}

fn character_appearance(index: usize) -> &'static str {
    [
        "年龄与生命阶段以剧本明确形态为准，脸型、眉眼、发型、体态、服装层次和随身配饰均应具体可辨。",
        "轮廓、目光、发型与体态体现人物行动习惯；服装颜色、剪裁、鞋履和配饰共同体现身份与当前阶段。",
        "皮肤状态、身形比例、衣料褶皱和使用痕迹真实自然，保留至少一件可辨认的随身饰物。",
    ][index % 3]
}

fn scene_structure(index: usize) -> &'static str {
    [
        "前景遮挡、中景行动区和远景环境形成明确纵深，主体建筑或自然环境具有可识别轮廓。",
        "入口、通道和核心区域的动线清楚，墙面、地面与主要陈设保持同一时代和使用尺度。",
        "空间保留一处可供人物停留或对峙的视觉中心，前后景层次清晰，方便持续作为分镜参考。",
    ][index % 3]
}

fn prop_details(index: usize) -> &'static str {
    [
        "颜色克制，尺寸符合手持或陈列用途；金属、木材、纸张或织物等材质边缘有细微磨损，表面装饰与故事背景一致。",
        "主体的形制和比例清楚，纹理、接缝、刻痕或封口等关键细节可近看识别，保留长期使用形成的自然痕迹。",
        "材质反光与阴影真实，边角、挂件或局部纹样有可辨特征；如需文字，仅保留剧情必要且不可读的简短符号。",
    ][index % 3]
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
        for alias in asset["aliases"].as_array().into_iter().flatten() {
            if let Some(alias) = alias.as_str().filter(|alias| !alias.is_empty()) {
                known.insert(alias.to_owned(), id.to_owned());
            }
        }
    }
    for node in &mut nodes {
        let mut ids: Vec<String> = Vec::new();
        for reference in node["reference_asset_ids"].as_array().into_iter().flatten() {
            let id = reference
                .as_str()
                .or_else(|| reference["asset_id"].as_str())
                .or_else(|| reference["asset_name"].as_str())
                .and_then(|value| {
                    known.get(value).or_else(|| {
                        known
                            .iter()
                            .filter(|(label, _)| value.contains(label.as_str()))
                            .max_by_key(|(label, _)| label.chars().count())
                            .map(|(_, id)| id)
                    })
                });
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
