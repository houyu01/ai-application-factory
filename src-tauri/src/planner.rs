//! Deterministic planners used offline and as validation-safe fallbacks around model calls.

use serde_json::{json, Value};

use crate::value::NOT_GENERATED;

#[path = "planner_assets.rs"]
mod asset_catalog;
#[path = "planner_asset_evidence.rs"]
mod asset_evidence;
#[cfg(test)]
#[path = "planner_asset_evidence_tests.rs"]
mod asset_evidence_tests;
#[path = "planner_asset_review.rs"]
mod asset_review;
#[cfg(test)]
#[path = "planner_asset_review_tests.rs"]
mod asset_review_tests;
#[path = "planner_game.rs"]
mod game_plan;
#[path = "planner_prop_evidence.rs"]
mod prop_evidence;
#[path = "planner_references.rs"]
mod reference_catalog;

pub(crate) use asset_catalog::extracted_assets;
pub(crate) use asset_evidence::AssetEvidence;
pub(crate) use asset_review::review_assets;
#[cfg(test)]
pub(crate) use game_plan::fallback_game_plan;
pub(crate) use game_plan::{
    game_expansion_prompt, game_graph_edge_feedback, game_graph_progress_checkpoint,
    game_graph_stage, game_graph_stage_checkpoint, game_graph_stage_prompt,
    game_graph_stage_response, merge_game_graph_stage_response, model_game_plan, GameGraphStage,
};
pub(crate) use reference_catalog::{
    key as reference_key, resolve_asset as resolve_reference_asset,
};

/// Validate the model's storyboard JSON before persistence, retaining deterministic defaults only for omitted optional fields.
#[allow(dead_code)]
pub fn model_drama_plan(
    response: &str,
    script: &str,
    style: &str,
    theme: &str,
    maximum_shot_chars: i64,
) -> Option<Value> {
    let parsed = parse_json_object(response)?;
    let episodes = parsed["episodes"]
        .as_array()?
        .iter()
        .enumerate()
        .filter_map(|(episode_index, episode)| {
            let shots = episode["shots"].as_array()?;
            let shots = shots
                .iter()
                .enumerate()
                .filter_map(|(shot_index, shot)| {
                    let original = shot["original_text"].as_str()?.trim();
                    (!original.is_empty()).then(|| {
                        json!({
                            "id":shot["id"].as_str().unwrap_or(&format!("shot_{}_{}", episode_index + 1, shot_index + 1)),
                            "title":shot["title"].as_str().filter(|text| !text.trim().is_empty()).unwrap_or("分镜"),
                            "original_text":clip(original, maximum_shot_chars.max(1) as usize),
                            "prompt":shot["prompt"].as_str().filter(|text| !text.trim().is_empty()).unwrap_or(&format!("{style}，围绕分镜连续动作生成镜头：{original}")),
                            "duration_seconds":shot["duration_seconds"].as_i64().unwrap_or(6).clamp(3, 15),
                        })
                    })
                })
                .collect::<Vec<_>>();
            (!shots.is_empty()).then(|| {
                json!({"name":episode["name"].as_str().filter(|name| !name.trim().is_empty()).unwrap_or(&format!("第{}集", episode_index + 1)),"shots":shots})
            })
        })
        .collect::<Vec<_>>();
    if episodes.is_empty() {
        return None;
    }
    let evidence = AssetEvidence::from_script(script);
    let assets = parsed["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|asset| {
            let kind = asset["type"].as_str()?;
            let name = evidence.canonical_name(
                kind,
                asset["name"].as_str()?.trim(),
                asset["source_evidence"].as_str().unwrap_or_default(),
            )?;
            let prompt = asset["prompt"].as_str()?.trim();
            (["character", "scene", "prop"].contains(&kind) && !name.is_empty() && !prompt.is_empty())
                .then(|| json!({"id":asset["id"],"type":kind,"name":name,"prompt":prompt,"status":NOT_GENERATED}))
        })
        .collect::<Vec<_>>();
    let assets = review_assets(script, theme, assets);
    Some(json!({"episodes":episodes,"assets":assets}))
}

/// Parse a model-produced rich-prompt document and retain only known asset references.
pub fn model_rich_prompt(response: &str, assets: &[Value]) -> Option<Vec<Value>> {
    let nodes = parse_json_object(response)?["nodes"].as_array()?.clone();
    let nodes = normalise_prompt_nodes(nodes, assets);
    let text = prompt_text(&nodes);
    (["场景：", "角色：", "风格：", "光线：", "位置："]
        .iter()
        .all(|section| text.contains(section))
        && text.contains("【镜头1"))
    .then_some(nodes)
}

pub(crate) fn parse_json_object(response: &str) -> Option<Value> {
    let trimmed = response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str(trimmed).ok().or_else(|| {
        let start = trimmed.find('{')?;
        let end = trimmed.rfind('}')?;
        serde_json::from_str(&trimmed[start..=end]).ok()
    })
}

/// Build a reviewable drama plan when a configured language model is unavailable or returns invalid JSON.
pub fn fallback_drama_plan(
    script: &str,
    style: &str,
    theme: &str,
    maximum_shot_chars: i64,
) -> Value {
    let clean = script.split_whitespace().collect::<String>();
    let count = ((clean.chars().count() + 79) / 80).clamp(2, 8);
    let segments = split_story(&clean, count);
    let titles = [
        "开场建立",
        "人物行动",
        "冲突发生",
        "信息揭示",
        "关系变化",
        "高潮推进",
        "结果显现",
        "收束结尾",
    ];
    let shots = segments.iter().enumerate().map(|(index, segment)| json!({
        "id":format!("shot_{:03}",index+1), "title":titles.get(index).unwrap_or(&"分镜"),
        "original_text":clip(segment, maximum_shot_chars as usize),
        "prompt":format!("{style}，{}，围绕这一段连续动作生成镜头：{segment}",titles.get(index).unwrap_or(&"分镜")),
        "duration_seconds":10,
    })).collect::<Vec<_>>();
    json!({"episodes":[{"name":"第1集","shots":shots}],"assets":extracted_assets(&clean,theme)})
}

/// Generate the rich-prompt baseline that can be edited before any image generation succeeds.
pub fn fallback_rich_prompt(project: &Value, shot: &Value, assets: &[Value]) -> Vec<Value> {
    fallback_rich_prompt_with_requests(project, shot, assets, &[])
}

/// Generate a deterministic prompt that preserves decomposition-time role-form choices.
pub fn fallback_rich_prompt_with_requests(
    project: &Value,
    shot: &Value,
    assets: &[Value],
    requests: &[Value],
) -> Vec<Value> {
    let duration = shot["duration_seconds"].as_i64().unwrap_or(10).clamp(3, 15);
    let introduction =
        first_appearance_instruction(shot["original_text"].as_str().unwrap_or_default());
    let requested = reference_catalog::planned_nodes(assets, requests);
    let references = if !requested.is_empty() {
        requested
    } else {
        assets
            .iter()
            .filter(|asset| {
                matches!(
                    asset["type"].as_str(),
                    Some("character") | Some("scene") | Some("prop")
                )
            })
            .take(3)
            .map(|asset| json!({"type":"reference","asset_id":asset["id"],"asset_type":asset["type"],"label":asset["name"],"image_url":asset["image_url"]}))
            .collect::<Vec<_>>()
    };
    let mut nodes = Vec::new();
    for (index, (label, kind)) in [("场景", "scene"), ("角色", "character"), ("道具", "prop")]
        .into_iter()
        .enumerate()
    {
        nodes.push(json!({"type":"text","text":format!("{}{}：", if index == 0 { "" } else { "\n" }, label)}));
        let mut first = true;
        for reference in references.iter().filter(|item| item["asset_type"] == kind) {
            if !first {
                nodes.push(json!({"type":"text","text":"、"}));
            }
            nodes.push(reference.clone());
            first = false;
        }
    }
    nodes.push(json!({"type":"text","text":format!("\n风格：{}，画幅：{}\n光线：根据剧情情绪组织。\n位置：保持人物、场景与道具空间关系稳定。\n【镜头1 | 时长{}s | 时间：日 外】\n动作：{}{}\n镜头：以一个连续镜头呈现动作变化，保持前后分镜衔接。\n【配音：旁白｜状态：待生成｜台词：根据原文自然表达】\n",project["style"].as_str().unwrap_or("真人风格"),project["ratio"].as_str().unwrap_or("9:16"),duration,shot["original_text"].as_str().unwrap_or_default(),introduction)}));
    normalise_prompt_nodes(nodes, assets)
}

fn first_appearance_instruction(source: &str) -> String {
    let Some(marker) = source.find("【人物首次出场：") else {
        return String::new();
    };
    let fragment = &source[marker..];
    let Some(end) = fragment.find('】') else {
        return String::new();
    };
    let description = &fragment[..end + '】'.len_utf8()];
    let name = description
        .trim_start_matches("【人物首次出场：")
        .split('｜')
        .next()
        .unwrap_or_default()
        .trim();
    (!name.is_empty()).then(|| format!("\n{description}\n【人物姓名标识｜姓名：{name}｜时长：1～2s｜位置：人物近旁且不遮挡脸部｜效果：快速淡入淡出】")).unwrap_or_default()
}

/// Restore referenced asset labels/images and assign per-asset visual mention numbers.
pub fn normalise_prompt_nodes(nodes: Vec<Value>, assets: &[Value]) -> Vec<Value> {
    let mut result = Vec::new();
    let mut mention = std::collections::HashMap::<String, i64>::new();
    let mut next = 1;
    for node in nodes {
        if node["type"] == "text" && node["text"].as_str().is_some_and(|value| !value.is_empty()) {
            result.push(json!({"type":"text","text":node["text"]}));
        }
        if node["type"] != "reference" {
            continue;
        }
        let id = node["asset_id"].as_str().unwrap_or_default();
        let variant_id = node["variant_id"].as_str().filter(|id| !id.is_empty());
        let asset = reference_catalog::resolve_asset(assets, id, variant_id);
        if asset.is_none() && node["asset_type"].as_str() != Some("placeholder") {
            continue;
        }
        let key = reference_catalog::key(id, variant_id);
        let number = *mention.entry(key).or_insert_with(|| {
            let current = next;
            next += 1;
            current
        });
        result.push(json!({"type":"reference","asset_id":id,"variant_id":variant_id,"asset_type":node["asset_type"].as_str().or_else(||asset.as_ref().and_then(|item|item["type"].as_str())).unwrap_or("prop"),"label":node["label"].as_str().or_else(||asset.as_ref().and_then(|item|item["name"].as_str())).unwrap_or("素材"),"image_url":asset.as_ref().and_then(|item|item["image_url"].as_str()),"mention_number":number}));
    }
    result
}

/// Convert rich prompt nodes into the plain prompt string submitted to all video providers.
pub fn prompt_text(nodes: &[Value]) -> String {
    nodes
        .iter()
        .map(|node| {
            if node["type"] == "reference" {
                format!(
                    "@图{}（{}）",
                    node["mention_number"].as_i64().unwrap_or(1),
                    node["label"].as_str().unwrap_or("素材")
                )
            } else {
                node["text"].as_str().unwrap_or_default().to_owned()
            }
        })
        .collect()
}

fn split_story(value: &str, count: usize) -> Vec<String> {
    if value.is_empty() {
        return vec![String::new(); count];
    }
    let mut units = value
        .split_inclusive(['，', '。', '！', '？', '；', ',', '!', '?', ';'])
        .filter(|unit| !unit.trim().is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if units.len() < count {
        let chars = value.chars().collect::<Vec<_>>();
        return (0..count)
            .filter_map(|index| {
                let start = chars.len() * index / count;
                let end = chars.len() * (index + 1) / count;
                (start < end).then(|| chars[start..end].iter().collect())
            })
            .collect();
    }
    let mut result = Vec::new();
    for index in 0..count {
        let remaining = count - index - 1;
        let take = (units.len() - remaining + count - index - 1) / (count - index);
        result.push(units.drain(..take).collect());
    }
    result
}
fn clip(value: &str, limit: usize) -> String {
    value.chars().take(limit.max(1)).collect()
}
#[cfg(test)]
mod tests {
    use super::first_appearance_instruction;

    #[test]
    fn fallback_prompt_preserves_first_appearance_name_card() {
        let prompt = first_appearance_instruction(
            "【人物首次出场：苏晚｜人物描述：年轻律师，短发，神情警惕】走进旧宅。",
        );
        assert!(prompt.contains("人物姓名标识｜姓名：苏晚｜时长：1～2s"));
    }
}
