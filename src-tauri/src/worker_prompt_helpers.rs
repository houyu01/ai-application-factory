//! Rich-prompt selection, prompt normalization, and deterministic quality parsing.

use serde_json::{json, Value};

use crate::{planner, value::SUCCEEDED};

pub(super) fn selected_ready_references(shot: &Value, assets: &[Value]) -> Vec<Value> {
    let mut selected = Vec::new();
    for node in shot["prompt_rich"].as_array().into_iter().flatten() {
        if let Some(asset) = planner::resolve_reference_asset(
            assets,
            node["asset_id"].as_str().unwrap_or_default(),
            node["variant_id"].as_str(),
        ) {
            add_ready_reference(&mut selected, &asset);
        }
    }
    let source = format!(
        "{} {}",
        shot["title"].as_str().unwrap_or_default(),
        shot["original_text"].as_str().unwrap_or_default()
    );
    for asset in assets {
        let name = asset["name"].as_str().unwrap_or_default();
        if !name.is_empty()
            && (source.contains(name)
                || name
                    .split(['·', '/', '、', '，', ' ', '：'])
                    .any(|term| term.chars().count() >= 2 && source.contains(term)))
        {
            add_ready_reference(&mut selected, asset);
        }
    }
    for kind in ["scene", "character", "prop"] {
        if !selected.iter().any(|asset| asset["type"] == kind) {
            if let Some(asset) = assets.iter().find(|asset| asset["type"] == kind) {
                add_ready_reference(&mut selected, asset);
            }
        }
    }
    selected
}

fn add_ready_reference(selected: &mut Vec<Value>, asset: &Value) {
    if asset["status"].as_str() == Some(SUCCEEDED)
        && asset["image_url"]
            .as_str()
            .is_some_and(|url| !url.is_empty())
        && matches!(
            asset["type"].as_str(),
            Some("scene") | Some("character") | Some("prop")
        )
        && !selected.iter().any(|item| {
            planner::reference_key(
                item["id"].as_str().unwrap_or_default(),
                item["variant_id"].as_str(),
            ) == planner::reference_key(
                asset["id"].as_str().unwrap_or_default(),
                asset["variant_id"].as_str(),
            )
        })
    {
        selected.push(asset.clone());
    }
}

pub(super) fn rich_prompt_request(
    project: &Value,
    shot: &Value,
    assets: &[Value],
    version: &str,
) -> String {
    format!(
        "模板版本：{version}\n短剧：{}\n分镜标题：{}\n分镜原文：{}\n风格：{}；画幅：{}；分辨率：{}；约束：{}\n候选素材（只能引用这些已生成图片的素材；带 variant_id 的角色条目就是一个独立角色形态，必须保留其 asset_id 和 variant_id 配对）：{}\n只返回 JSON：{{\"nodes\":[{{\"type\":\"text\",\"text\":\"...\"}},{{\"type\":\"reference\",\"asset_id\":\"角色素材ID\",\"variant_id\":\"可选的角色形态ID\",\"asset_type\":\"character|scene|prop|placeholder\",\"label\":\"素材或角色形态名称\"}}]}}。每个参考节点必须紧随其对应的“场景：”“角色：”或“道具：”字段，绝不可在配音后集中罗列。若当前分镜中的人物处于候选目录标注的幼年、成年、伤病、变身、换装或其他形态，必须引用该条目的 variant_id，不能退回引用角色基础形态。若分镜原文包含“【人物首次出场：当前名字｜人物描述：…】”，必须保留描述，并在人物首次清晰入画的对应镜头写入“【人物姓名标识｜姓名：当前角色素材的 name｜时长：1～2s｜位置：人物近旁且不遮挡脸部｜效果：快速淡入淡出】”；姓名优先使用候选素材中的当前 name，标识不是字幕，即使 subtitles 为 false 也必须保留且只出现一次。文字依次包含场景、角色、道具、风格、光线、位置、{}和每个镜头对应的配音；镜头头部必须为【镜头N | 时长Xs | 时间：日 外】。不要写图片 URL 或技术标识。",
        project["name"].as_str().unwrap_or("短剧"), shot["title"].as_str().unwrap_or("分镜"), shot["original_text"].as_str().unwrap_or_default(), project["style"].as_str().unwrap_or("真人风格"), project["ratio"].as_str().unwrap_or("9:16"), project["resolution"].as_str().unwrap_or("720p"), project["shot_constraints"], format!("{:?}", assets), if version == "v2" { "一个完整连续长镜头" } else { "2 到 3 个连续镜头" },
    )
}

pub(super) fn append_missing_references(mut nodes: Vec<Value>, assets: &[Value]) -> Vec<Value> {
    let missing = assets
        .iter()
        .filter(|asset| {
            !nodes.iter().any(|node| {
                node["type"] == "reference"
                    && planner::reference_key(
                        node["asset_id"].as_str().unwrap_or_default(),
                        node["variant_id"].as_str(),
                    ) == planner::reference_key(
                        asset["id"].as_str().unwrap_or_default(),
                        asset["variant_id"].as_str(),
                    )
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    for asset in missing.into_iter().rev() {
        insert_missing_reference(&mut nodes, asset);
    }
    planner::normalise_prompt_nodes(nodes, assets)
}

fn insert_missing_reference(nodes: &mut Vec<Value>, asset: Value) {
    let kind = asset["type"].as_str().unwrap_or("prop");
    let label = match kind {
        "scene" => "场景",
        "character" => "角色",
        "prop" => "道具",
        _ => return,
    };
    let marker = format!("{label}：");
    let reference = json!({"type":"reference","asset_id":asset["id"],"variant_id":asset["variant_id"],"asset_type":kind,"label":asset["name"],"image_url":asset["image_url"]});
    if let Some(index) = nodes.iter().position(|node| {
        node["text"]
            .as_str()
            .is_some_and(|text| text.contains(&marker))
    }) {
        let text = nodes[index]["text"].as_str().unwrap_or_default().to_owned();
        let position = text.find(&marker).unwrap_or_default() + marker.len();
        let before = &text[..position];
        let after = &text[position..];
        nodes[index] = json!({"type":"text","text":before});
        nodes.insert(index + 1, reference);
        if nodes
            .get(index + 2)
            .is_some_and(|node| node["type"] == "reference")
        {
            nodes.insert(index + 2, json!({"type":"text","text":"、"}));
        }
        if !after.is_empty() {
            nodes.insert(index + 2, json!({"type":"text","text":after}));
        }
        return;
    }
    let position = nodes
        .iter()
        .position(|node| {
            node["text"]
                .as_str()
                .is_some_and(|text| text.contains("风格："))
        })
        .unwrap_or(nodes.len());
    nodes.insert(
        position,
        json!({"type":"text","text":format!("\n{label}：")}),
    );
    nodes.insert(position + 1, reference);
}

pub(super) fn filter_disallowed_sections(nodes: Vec<Value>, project: &Value) -> Vec<Value> {
    let subtitles = project["shot_constraints"]["subtitles"]
        .as_bool()
        .unwrap_or(false);
    let music = project["shot_constraints"]["background_music"]
        .as_bool()
        .unwrap_or(false);
    nodes
        .into_iter()
        .filter_map(|mut node| {
            if node["type"] != "text" {
                return Some(node);
            }
            let mut text = node["text"].as_str().unwrap_or_default().to_owned();
            if !subtitles {
                text = strip_labelled_sections(&text, &["字幕"]);
            }
            if !music {
                text = strip_labelled_sections(&text, &["背景音乐", "配乐", "BGM"]);
            }
            (!text.is_empty()).then(|| {
                node["text"] = json!(text);
                node
            })
        })
        .collect()
}

fn strip_labelled_sections(text: &str, labels: &[&str]) -> String {
    let lines = text
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !labels.iter().any(|label| {
                trimmed.starts_with(label)
                    && trimmed[label.len()..].trim_start().starts_with(['：', ':'])
            })
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut result = String::new();
    let mut rest = lines.as_str();
    while let Some(start) = rest.find('【') {
        result.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(end) = after.find('】') else {
            result.push_str(after);
            return result;
        };
        let closing_end = end + '】'.len_utf8();
        let block = &after[..closing_end];
        if !labels.iter().any(|label| block.contains(label)) {
            result.push_str(block);
        }
        rest = &after[closing_end..];
    }
    result.push_str(rest);
    result
}

pub(super) fn structured_from_prompt(project: &Value, shot: &Value, nodes: &[Value]) -> Value {
    let text = planner::prompt_text(nodes);
    let references = nodes.iter().filter(|node| node["type"] == "reference").map(|node| json!({"asset_id":node["asset_id"],"variant_id":node["variant_id"],"asset_type":node["asset_type"],"label":node["label"]})).collect::<Vec<_>>();
    let camera_shots = text.split("【镜头").skip(1).filter_map(|part| {
        let (header, description) = part.split_once('】')?;
        let duration = header.split("时长").nth(1)?.trim_start().chars().take_while(char::is_ascii_digit).collect::<String>().parse::<i64>().ok()?;
        Some(json!({"index":header.chars().take_while(char::is_ascii_digit).collect::<String>().parse::<i64>().unwrap_or(1),"duration_seconds":duration,"time":header.split("时间：").nth(1).unwrap_or_default().trim(),"description":description.split('【').next().unwrap_or_default().trim()}))
    }).collect::<Vec<_>>();
    let voice_blocks = text.split("【配音：").skip(1).filter_map(|part| part.split_once('】').map(|(voice, _)| {
        let field = |name: &str| voice.split('｜').find_map(|item| item.strip_prefix(name).map(str::trim)).unwrap_or_default();
        json!({"speaker":voice.split('｜').next().unwrap_or_default(),"voice_id":field("VoiceID："),"state":field("状态："),"emotion":field("情绪："),"dialogue":field("台词：")})
    })).collect::<Vec<_>>();
    let ids = |kind: &str| {
        references
            .iter()
            .filter(|item| item["asset_type"] == kind)
            .map(|item| item["asset_id"].clone())
            .collect::<Vec<_>>()
    };
    json!({"scene_reference_ids":ids("scene"),"character_reference_ids":ids("character"),"prop_reference_ids":ids("prop"),"placeholder_reference_ids":ids("placeholder"),"references":references,"camera_shots":camera_shots,"shot_count":camera_shots.len(),"duration_seconds":camera_shots.iter().map(|item| item["duration_seconds"].as_i64().unwrap_or(0)).sum::<i64>().max(shot["duration_seconds"].as_i64().unwrap_or(0)),"voice_blocks":voice_blocks,"has_dialogue":voice_blocks.iter().any(|item| !matches!(item["dialogue"].as_str(), Some("") | Some("（无新增台词）") | Some("(无新增台词)"))),"sections":{"scene":text.contains("场景："),"characters":text.contains("角色："),"style":text.contains("风格："),"lighting":text.contains("光线："),"position":text.contains("位置：")},"style":project["style"],"ratio":project["ratio"],"resolution":project["resolution"],"constraints":project["shot_constraints"],"prompt_template_version":shot["prompt_template_version"],"source_text":shot["original_text"]})
}

pub(super) fn issue(code: &str, severity: &str, message: &str, field: &str) -> Value {
    json!({"code":code,"severity":severity,"message":message,"field":field})
}
