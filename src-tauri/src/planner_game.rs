//! Interactive-video game expansion, graph prompts, and validation.

use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::{json, Map, Value};

use crate::{
    repository::game_validation::GAME_VIDEO_DURATION_RANGE,
    value::{ground_game_video_prompt, NOT_GENERATED},
};

use super::{parse_json_object, review_assets};

#[path = "planner_game_checkpoint.rs"]
mod checkpoint;
#[cfg(test)]
#[path = "planner_game_checkpoint_tests.rs"]
mod checkpoint_tests;
#[path = "planner_game_choices.rs"]
mod choices;
#[path = "planner_game_expansion.rs"]
mod expansion;
#[cfg(test)]
#[path = "planner_game_fallback.rs"]
mod fallback;
#[path = "planner_game_materials.rs"]
mod materials;
#[path = "planner_game_stages.rs"]
mod stages;

pub(crate) use checkpoint::game_graph_progress_checkpoint;
#[cfg(test)]
pub(crate) use checkpoint::{merge_game_graph_resume, resume_game_graph_prompt};
#[cfg(test)]
use choices::CHOICE_LABEL_CONTRACT;
use choices::{choice_label_key, is_meaningful_choice_label};
pub(crate) use expansion::game_expansion_prompt;
#[cfg(test)]
pub(crate) use fallback::fallback_game_plan;
#[cfg(test)]
use materials::GAME_ASSET_PROMPT_CONTRACT;
use materials::{normalize_assets, resolve_node_references};
pub(crate) use stages::{
    game_graph_edge_feedback, game_graph_stage, game_graph_stage_checkpoint,
    game_graph_stage_prompt, game_graph_stage_response, merge_game_graph_stage_response,
    GameGraphStage,
};

/// Build the former all-in-one graph instruction for planner regression coverage.
#[cfg(test)]
pub(crate) fn game_graph_prompt(game: &Value, expanded_script: &str) -> String {
    let branch_min = integer(game, "branch_min", 2, 2, 4);
    let branch_max = integer(game, "branch_max", 4, branch_min, 4);
    let node_limit = integer(game, "node_script_max_chars", 400, 1, 1_000_000);
    let configured_duration_min = integer(game, "node_duration_min", 5, 1, 600);
    let configured_duration_max =
        integer(game, "node_duration_max", 15, configured_duration_min, 600);
    let duration_min = configured_duration_min.clamp(
        *GAME_VIDEO_DURATION_RANGE.start(),
        *GAME_VIDEO_DURATION_RANGE.end(),
    );
    let duration_max =
        configured_duration_max.clamp(duration_min, *GAME_VIDEO_DURATION_RANGE.end());
    let prompt = format!(
        "根据扩写剧本生成互动视频游戏图谱。扩写稿是已设计好的分支剧本，必须以其中的“剧情段 ID、玩家抉择、触发条件、状态变化、前往 ID、结局 ID”为事实来源拆分，绝不可把它压平为单线小说或改写掉已有条件。若文本前部保留了旧版单线正文，仅将带有“【互动剧本总览】”“【剧情段】”“【玩家抉择】”“【结局】”标记的结构化区块作为分支事实来源。只能返回一个合法 JSON 对象：{{\"assets\":[...],\"nodes\":[...],\"edges\":[...]}}。\n\n节点结构：{{\"id\":\"唯一标识\",\"node_type\":\"start|normal|success|failure\",\"title\":\"标题\",\"original_text\":\"该视频节点的剧情正文\",\"prompt\":\"可直接生成视频的提示词\",\"reference_asset_ids\":[\"实际出现的素材 id\"],\"duration_seconds\":整数}}。边结构：{{\"id\":\"唯一标识\",\"source_node_id\":\"来源节点\",\"target_node_id\":\"目标节点\",\"option_text\":\"玩家选择文案\",\"sort_order\":整数,\"conditions\":{{\"set\":{{\"状态键\":true}},\"requires\":{{\"状态键\":true}}}}}}。素材结构：{{\"id\":\"唯一标识\",\"type\":\"character|scene|prop\",\"name\":\"名称\",\"prompt\":\"可复用视觉描述\"}}。\n\n映射规则：\n1. 每个“【剧情段 Sxx】”至少映射为一个 start 或 normal 节点；每个“【结局 Exx｜成功/失败】”必须映射为对应类型的终局节点。每条“选择”必须成为从所属剧情段出发的 edge；“前往”指定 edge 的目标；“触发条件”写入 conditions.requires；“状态变化”写入 conditions.set。不得删除、合并或臆造剧本已明确的选择、条件、状态与终局因果。\n2. 必须恰好 1 个 start、{success} 个 success 和 {failure} 个 failure；success/failure 没有出边。若剧本中存在更多同类结局，保留最完整且条件最不同的 {success}/{failure} 个；若不足，基于既有剧情补齐，不得破坏已明确的分支条件。\n3. 图必须是有向无环图（DAG），不是按层铺满的 N 叉树：不同路径可以在任意 normal 节点汇合；成功和失败结局可位于不同深度，绝不可把所有结局集中到最后一层。每次玩家选择都可以直接进入 failure，包括 start 的首次选择；应按剧情风险分散失败结局，而不是延后到最终抉择。\n4. start 节点必须有 {branch_min} 至 {branch_max} 条选择边。normal 节点若承载玩家抉择，也必须有 {branch_min} 至 {branch_max} 条选择边；若只是无抉择的剧情承接，可恰有 1 条线性后继边。所有节点从 start 可达，所有非终局节点可到达某个终局。\n5. 保留剧本中的跨分支状态影响：早期选择的状态变化写入 conditions.set；后续剧情段或结局的触发条件写入 conditions.requires。即使不同路径随后汇合到同一视频节点，也不能丢失状态读取与不同后果。状态值只能为字符串、数字或布尔值，不能把状态写入视频节点。\n6. 每条边的选项必须使用剧本中的玩家选择，并紧密承接来源视频的最后动作/信息，明确导向目标视频的不同后果；不能使用“选项 A”“继续”“路径 1”这类泛化文案。\n7. 每条 original_text 不超过 {node_limit} 个中文字符，去除空白后必须在全图唯一；duration_seconds 必须为 {duration_min} 至 {duration_max} 秒。\n8. 每条 prompt 必须按“场景：”“角色：”“道具：”“风格：”“光线：”“位置：”“镜头：”“前序承接：”“选择后果：”分段，使用 @图说明 reference_asset_ids 中实际素材的参考作用；风格为“{style}”，分辨率为“{resolution}”。去除空白后，任意两条提示词不得完全相同。\n9. 先完整拆出真正可复用的角色、场景、道具；每项素材 prompt 第一行必须是“叙述背景主题：互动游戏”，并写清稳定视觉锚点。角色含身份、年龄/性别、至少三项可观察行为、外貌服装配饰；场景含剧情用途、空间结构、陈设、色调光线；道具含叙事用途、颜色材质、尺寸形制、纹理磨损。不得生成图片 URL。\n10. 主人公不可缺席：先读取“【互动剧本总览】”的主人公并生成其独立 character 素材；若该字段缺失，在生成角色前按玩家目标、主要行动和结局后果判定唯一主人公，仍无法判定时补全一名有真实姓名的主人公并写入 start 节点。主人公必须出现在 start 节点、关键分支和所有结局的剧情或结果中，不能用“玩家”“主角”或群像代替；图谱不得只有场景、道具或配角。\n11. 只配置后续人工选择的首尾帧、占位图和封面，不要生成任何图片。\n\n扩写剧本：\n{expanded_script}",
        success = integer(game, "success_ending_count", 2, 1, 100),
        failure = integer(game, "failure_ending_count", 12, 1, 200),
        style = game["style"].as_str().unwrap_or("真人风格"),
        resolution = game["resolution"].as_str().unwrap_or("720p"),
    );
    format!(
        "{prompt}\n\n{CHOICE_LABEL_CONTRACT}\n\n{}\n\n{GAME_ASSET_PROMPT_CONTRACT}",
        choices::VIDEO_NODE_TRANSITION_CONTRACT
    )
}

/// Validate and normalize a model graph before it reaches SQLite.
pub(crate) fn model_game_plan(response: &str, game: &Value) -> Option<Value> {
    let parsed = parse_json_object(response)?;
    let node_limit = integer(game, "node_script_max_chars", 400, 1, 1_000_000) as usize;
    let configured_duration_min = integer(game, "node_duration_min", 5, 1, 600);
    let configured_duration_max =
        integer(game, "node_duration_max", 15, configured_duration_min, 600);
    let duration_min = configured_duration_min.clamp(
        *GAME_VIDEO_DURATION_RANGE.start(),
        *GAME_VIDEO_DURATION_RANGE.end(),
    );
    let duration_max =
        configured_duration_max.clamp(duration_min, *GAME_VIDEO_DURATION_RANGE.end());
    let mut ids = HashSet::new();
    let mut kinds = HashMap::new();
    let mut original_texts = HashSet::new();
    let mut video_prompts = HashSet::new();
    let mut nodes = Vec::new();
    for node in parsed["nodes"].as_array()? {
        let id = node["id"].as_str()?.trim();
        let kind = node["node_type"].as_str()?.trim();
        let title = node["title"].as_str()?.trim();
        let original = node["original_text"].as_str()?.trim();
        let prompt = node["prompt"].as_str()?.trim();
        if id.is_empty()
            || title.is_empty()
            || original.is_empty()
            || prompt.is_empty()
            || !["start", "normal", "success", "failure"].contains(&kind)
            || !ids.insert(id.to_owned())
        {
            return None;
        }
        kinds.insert(id.to_owned(), kind.to_owned());
        let original = clip(original, node_limit);
        let video_prompt = ground_game_video_prompt(prompt, &original);
        if !original_texts.insert(node_text_key(&original))
            || !video_prompts.insert(node_text_key(&video_prompt))
        {
            return None;
        }
        nodes.push(json!({
            "id": id,
            "node_type": kind,
            "title": clip(title, 80),
            "original_text": original,
            "prompt": video_prompt,
            "reference_asset_ids": node["reference_asset_ids"].as_array().cloned().unwrap_or_default(),
            "duration_seconds": node["duration_seconds"].as_i64().unwrap_or(duration_min).clamp(duration_min, duration_max),
            "status": NOT_GENERATED,
            "video_history": [],
        }));
    }
    if nodes.is_empty() || exact_endings(&kinds, game).is_none() {
        return None;
    }
    let edges = normalize_edges(parsed["edges"].as_array()?, &ids)?;
    graph_is_playable(&kinds, &edges, game)?;
    place_nodes(&mut nodes, &edges);
    let assets = normalize_assets(
        parsed["assets"].as_array(),
        &expanded_or_source(game),
        game["style"].as_str().unwrap_or("真人风格"),
    );
    let nodes = resolve_node_references(nodes, &assets);
    Some(json!({"assets":assets,"nodes":nodes,"edges":edges}))
}

/// Compare creator-visible node content without treating whitespace-only changes as originality.
fn node_text_key(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}
fn exact_endings(kinds: &HashMap<String, String>, game: &Value) -> Option<()> {
    let count = |kind| {
        kinds
            .values()
            .filter(|value| value.as_str() == kind)
            .count() as i64
    };
    (count("start") == 1
        && count("success") == integer(game, "success_ending_count", 2, 1, 100)
        && count("failure") == integer(game, "failure_ending_count", 12, 1, 200))
    .then_some(())
}
fn normalize_edges(raw: &[Value], ids: &HashSet<String>) -> Option<Vec<Value>> {
    let mut edge_ids = HashSet::new();
    let mut sorts = HashMap::<String, i64>::new();
    let mut choices = HashMap::<String, HashSet<String>>::new();
    raw.iter()
        .map(|edge| {
            let id = edge["id"].as_str()?.trim();
            let source = edge["source_node_id"].as_str()?.trim();
            let target = edge["target_node_id"].as_str()?.trim();
            let option = edge["option_text"].as_str()?.trim();
            let choice_key = choice_label_key(option);
            if id.is_empty()
                || source == target
                || option.is_empty()
                || !is_meaningful_choice_label(option)
                || !ids.contains(source)
                || !ids.contains(target)
                || !edge_ids.insert(id.to_owned())
                || !choices
                    .entry(source.to_owned())
                    .or_default()
                    .insert(choice_key)
            {
                return None;
            }
            let sort = sorts.entry(source.to_owned()).or_insert(0);
            *sort += 1;
            Some(json!({"id":id,"source_node_id":source,"target_node_id":target,"option_text":clip(option,80),"sort_order":*sort,"conditions":normalize_edge_conditions(edge.get("conditions"))?}))
        })
        .collect()
}

fn normalize_edge_conditions(value: Option<&Value>) -> Option<Value> {
    let Some(value) = value else {
        return Some(json!({}));
    };
    let raw = value.as_object()?;
    if raw.keys().any(|key| key != "requires" && key != "set") {
        return None;
    }
    let mut normalized = Map::new();
    for kind in ["requires", "set"] {
        let Some(entries) = raw.get(kind) else {
            continue;
        };
        let entries = entries.as_object()?;
        let mut values = Map::new();
        for (key, value) in entries {
            if !valid_state_key(key) || !valid_state_value(value) {
                return None;
            }
            values.insert(key.to_owned(), value.clone());
        }
        if !values.is_empty() {
            normalized.insert(kind.to_owned(), Value::Object(values));
        }
    }
    Some(Value::Object(normalized))
}

fn valid_state_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}

fn valid_state_value(value: &Value) -> bool {
    value.is_string() || value.is_number() || value.is_boolean()
}

fn graph_is_playable(kinds: &HashMap<String, String>, edges: &[Value], game: &Value) -> Option<()> {
    let mut outgoing = HashMap::<String, Vec<String>>::new();
    let mut incoming = HashMap::<String, Vec<String>>::new();
    for edge in edges {
        let source = edge["source_node_id"].as_str()?.to_owned();
        let target = edge["target_node_id"].as_str()?.to_owned();
        outgoing
            .entry(source.clone())
            .or_default()
            .push(target.clone());
        incoming.entry(target).or_default().push(source);
    }
    let minimum = integer(game, "branch_min", 2, 2, 4) as usize;
    let maximum = integer(game, "branch_max", 4, minimum as i64, 4) as usize;
    for (id, kind) in kinds {
        let count = outgoing.get(id).map_or(0, Vec::len);
        let valid_count = match kind.as_str() {
            "success" | "failure" => count == 0,
            "start" => (minimum..=maximum).contains(&count),
            "normal" => count == 1 || (minimum..=maximum).contains(&count),
            _ => false,
        };
        if !valid_count {
            return None;
        }
    }
    let start = kinds
        .iter()
        .find_map(|(id, kind)| (kind == "start").then_some(id))?;
    let reached = traversal(start, &outgoing);
    if reached.len() != kinds.len() {
        return None;
    }
    let endings = kinds
        .iter()
        .filter(|(_, kind)| ["success", "failure"].contains(&kind.as_str()))
        .map(|(id, _)| id.to_owned())
        .collect::<Vec<_>>();
    let mut reaches_end = HashSet::new();
    for ending in endings {
        reaches_end.extend(traversal(&ending, &incoming));
    }
    (reaches_end.len() == kinds.len() && !has_cycle(kinds, &outgoing)).then_some(())
}

fn traversal(start: &str, links: &HashMap<String, Vec<String>>) -> HashSet<String> {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([start.to_owned()]);
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id.clone()) {
            continue;
        }
        queue.extend(links.get(&id).into_iter().flatten().cloned());
    }
    seen
}

fn has_cycle(kinds: &HashMap<String, String>, outgoing: &HashMap<String, Vec<String>>) -> bool {
    fn visit(
        id: &str,
        outgoing: &HashMap<String, Vec<String>>,
        permanent: &mut HashSet<String>,
        visiting: &mut HashSet<String>,
    ) -> bool {
        if permanent.contains(id) {
            return false;
        }
        if !visiting.insert(id.to_owned()) {
            return true;
        }
        let cycle = outgoing
            .get(id)
            .into_iter()
            .flatten()
            .any(|next| visit(next, outgoing, permanent, visiting));
        visiting.remove(id);
        permanent.insert(id.to_owned());
        cycle
    }
    let mut permanent = HashSet::new();
    let mut visiting = HashSet::new();
    kinds
        .keys()
        .any(|id| visit(id, outgoing, &mut permanent, &mut visiting))
}

fn place_nodes(nodes: &mut [Value], edges: &[Value]) {
    let mut levels = HashMap::<String, i64>::new();
    let mut outgoing = HashMap::<String, Vec<String>>::new();
    let mut queue = VecDeque::new();
    if let Some(start) = nodes.iter().find(|node| node["node_type"] == "start") {
        let id = start["id"].as_str().unwrap_or_default().to_owned();
        levels.insert(id.clone(), 0);
        queue.push_back(id);
    }
    for edge in edges {
        if let (Some(source), Some(target)) = (
            edge["source_node_id"].as_str(),
            edge["target_node_id"].as_str(),
        ) {
            outgoing
                .entry(source.to_owned())
                .or_default()
                .push(target.to_owned());
        }
    }
    while let Some(id) = queue.pop_front() {
        let level = levels.get(&id).copied().unwrap_or(0);
        for target in outgoing.get(&id).into_iter().flatten() {
            if levels.get(target).map_or(true, |known| *known < level + 1) {
                levels.insert(target.to_owned(), level + 1);
                queue.push_back(target.to_owned());
            }
        }
    }
    let mut rows = HashMap::<i64, i64>::new();
    for node in nodes {
        let level = levels
            .get(node["id"].as_str().unwrap_or_default())
            .copied()
            .unwrap_or(0);
        let row = rows.entry(level).or_insert(0);
        node["position_x"] = json!(80 + level * 360);
        node["position_y"] = json!(80 + *row * 190);
        *row += 1;
    }
}
fn expanded_or_source(game: &Value) -> String {
    game["expanded_script"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| game["script"].as_str().unwrap_or_default())
        .to_owned()
}

fn integer(game: &Value, key: &str, default: i64, minimum: i64, maximum: i64) -> i64 {
    game[key]
        .as_i64()
        .unwrap_or(default)
        .clamp(minimum, maximum)
}

fn clip(value: &str, limit: usize) -> String {
    value.chars().take(limit.max(1)).collect()
}

#[cfg(test)]
#[path = "planner_game_tests.rs"]
mod tests;
