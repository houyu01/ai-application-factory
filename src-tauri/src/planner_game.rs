//! Interactive-video game expansion, graph prompts, validation, and offline fallback plans.

use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::{json, Map, Value};

use crate::{
    repository::game_validation::GAME_VIDEO_DURATION_RANGE,
    value::{ground_game_video_prompt, NOT_GENERATED},
};

use super::{extracted_assets, parse_json_object};

#[path = "planner_game_expansion.rs"]
mod expansion;
#[path = "planner_game_fallback.rs"]
mod fallback;
#[path = "planner_game_materials.rs"]
mod materials;

use materials::{asset_prompt, resolve_node_references};
pub(crate) use {
    expansion::{fallback_game_expansion, game_expansion_prompt},
    fallback::fallback_game_plan,
};

/// Build the strict DAG-planning instruction consumed together with the bundled branch-planner skill.
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
    format!(
        "根据扩写剧本生成互动视频游戏图谱。扩写稿是已设计好的分支剧本，必须以其中的“剧情段 ID、玩家抉择、触发条件、状态变化、前往 ID、结局 ID”为事实来源拆分，绝不可把它压平为单线小说或改写掉已有条件。若文本前部保留了旧版单线正文，仅将带有“【互动剧本总览】”“【剧情段】”“【玩家抉择】”“【结局】”标记的结构化区块作为分支事实来源。只能返回一个合法 JSON 对象：{{\"assets\":[...],\"nodes\":[...],\"edges\":[...]}}。\n\n节点结构：{{\"id\":\"唯一标识\",\"node_type\":\"start|normal|success|failure\",\"title\":\"标题\",\"original_text\":\"该视频节点的剧情正文\",\"prompt\":\"可直接生成视频的提示词\",\"reference_asset_ids\":[\"实际出现的素材 id\"],\"duration_seconds\":整数}}。边结构：{{\"id\":\"唯一标识\",\"source_node_id\":\"来源节点\",\"target_node_id\":\"目标节点\",\"option_text\":\"玩家选择文案\",\"sort_order\":整数,\"conditions\":{{\"set\":{{\"状态键\":true}},\"requires\":{{\"状态键\":true}}}}}}。素材结构：{{\"id\":\"唯一标识\",\"type\":\"character|scene|prop\",\"name\":\"名称\",\"prompt\":\"可复用视觉描述\"}}。\n\n映射规则：\n1. 每个“【剧情段 Sxx】”至少映射为一个 start 或 normal 节点；每个“【结局 Exx｜成功/失败】”必须映射为对应类型的终局节点。每条“选择”必须成为从所属剧情段出发的 edge；“前往”指定 edge 的目标；“触发条件”写入 conditions.requires；“状态变化”写入 conditions.set。不得删除、合并或臆造剧本已明确的选择、条件、状态与终局因果。\n2. 必须恰好 1 个 start、{success} 个 success 和 {failure} 个 failure；success/failure 没有出边。若剧本中存在更多同类结局，保留最完整且条件最不同的 {success}/{failure} 个；若不足，基于既有剧情补齐，不得破坏已明确的分支条件。\n3. 图必须是有向无环图（DAG），不是按层铺满的 N 叉树：不同路径可以在任意 normal 节点汇合；成功和失败结局可位于不同深度，绝不可把所有结局集中到最后一层。每次玩家选择都可以直接进入 failure，包括 start 的首次选择；应按剧情风险分散失败结局，而不是延后到最终抉择。\n4. start 节点必须有 {branch_min} 至 {branch_max} 条选择边。normal 节点若承载玩家抉择，也必须有 {branch_min} 至 {branch_max} 条选择边；若只是无抉择的剧情承接，可恰有 1 条线性后继边。所有节点从 start 可达，所有非终局节点可到达某个终局。\n5. 保留剧本中的跨分支状态影响：早期选择的状态变化写入 conditions.set；后续剧情段或结局的触发条件写入 conditions.requires。即使不同路径随后汇合到同一视频节点，也不能丢失状态读取与不同后果。状态值只能为字符串、数字或布尔值，不能把状态写入视频节点。\n6. 每条边的选项必须使用剧本中的玩家选择，并紧密承接来源视频的最后动作/信息，明确导向目标视频的不同后果；不能使用“选项 A”“继续”“路径 1”这类泛化文案。\n7. 每条 original_text 不超过 {node_limit} 个中文字符；duration_seconds 必须为 {duration_min} 至 {duration_max} 秒。\n8. 每条 prompt 必须按“场景：”“角色：”“道具：”“风格：”“光线：”“位置：”“镜头：”“前序承接：”“选择后果：”分段，使用 @图说明 reference_asset_ids 中实际素材的参考作用；风格为“{style}”，分辨率为“{resolution}”。\n9. 先完整拆出真正可复用的角色、场景、道具；每项素材 prompt 第一行必须是“叙述背景主题：互动游戏”，并写清稳定视觉锚点。角色含身份、年龄/性别、至少三项可观察行为、外貌服装配饰；场景含剧情用途、空间结构、陈设、色调光线；道具含叙事用途、颜色材质、尺寸形制、纹理磨损。不得生成图片 URL。\n10. 只配置后续人工选择的首尾帧、占位图和封面，不要生成任何图片。\n\n扩写剧本：\n{expanded_script}",
        success = integer(game, "success_ending_count", 2, 1, 100),
        failure = integer(game, "failure_ending_count", 30, 1, 200),
        style = game["style"].as_str().unwrap_or("真人风格"),
        resolution = game["resolution"].as_str().unwrap_or("720p"),
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
        nodes.push(json!({
            "id": id,
            "node_type": kind,
            "title": clip(title, 80),
            "original_text": original,
            "prompt": ground_game_video_prompt(prompt, &original),
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
    let assets = normalize_assets(parsed["assets"].as_array(), game);
    let nodes = resolve_node_references(nodes, &assets);
    Some(json!({"assets":assets,"nodes":nodes,"edges":edges}))
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
        && count("failure") == integer(game, "failure_ending_count", 30, 1, 200))
    .then_some(())
}

fn normalize_edges(raw: &[Value], ids: &HashSet<String>) -> Option<Vec<Value>> {
    let mut edge_ids = HashSet::new();
    let mut sorts = HashMap::<String, i64>::new();
    raw.iter()
        .map(|edge| {
            let id = edge["id"].as_str()?.trim();
            let source = edge["source_node_id"].as_str()?.trim();
            let target = edge["target_node_id"].as_str()?.trim();
            let option = edge["option_text"].as_str()?.trim();
            if id.is_empty()
                || source == target
                || option.is_empty()
                || !ids.contains(source)
                || !ids.contains(target)
                || !edge_ids.insert(id.to_owned())
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
        node["position_x"] = json!(80 + level * 280);
        node["position_y"] = json!(80 + *row * 130);
        *row += 1;
    }
}

fn normalize_assets(raw: Option<&Vec<Value>>, game: &Value) -> Vec<Value> {
    let mut assets = raw.into_iter().flatten().filter_map(|asset| {
        let kind = asset["type"].as_str()?;
        let name = asset["name"].as_str()?.trim();
        let prompt = asset["prompt"].as_str()?.trim();
        (["character", "scene", "prop"].contains(&kind) && !name.is_empty() && !prompt.is_empty())
            .then(|| json!({"id":asset["id"].as_str().unwrap_or(name),"type":kind,"name":clip(name,80),"prompt":asset_prompt(prompt),"status":NOT_GENERATED}))
    }).collect::<Vec<_>>();
    let fallback_candidates = if assets.is_empty() {
        fallback_assets(&expanded_or_source(game))
    } else {
        extracted_assets(&expanded_or_source(game), "互动游戏")
    };
    for fallback in fallback_candidates {
        let duplicate = assets
            .iter()
            .any(|asset| asset["type"] == fallback["type"] && asset["name"] == fallback["name"]);
        if !duplicate {
            assets.push(fallback);
        }
    }
    assets
}

fn fallback_assets(script: &str) -> Vec<Value> {
    let assets = extracted_assets(script, "互动游戏");
    if assets.is_empty() {
        vec![
            json!({"id":"character_001","type":"character","name":"主角","prompt":"叙述背景主题：互动游戏\n互动游戏主角，稳定外貌、服装和情绪特征。","status":NOT_GENERATED}),
        ]
    } else {
        assets
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
mod tests {
    use serde_json::json;

    use super::{fallback_game_plan, game_graph_prompt, model_game_plan};

    #[test]
    fn graph_prompt_preserves_branch_screenplay_semantics() {
        let prompt = game_graph_prompt(
            &json!({"success_ending_count":1,"failure_ending_count":1}),
            "【剧情段 S01｜开始】\n【玩家抉择】\n触发条件：token=true\n状态变化：token=true\n前往：E01",
        );

        for required in [
            "触发条件”写入 conditions.requires",
            "状态变化”写入 conditions.set",
            "前往”指定 edge 的目标",
            "不得删除、合并或臆造",
        ] {
            assert!(prompt.contains(required), "missing {required}");
        }
    }

    #[test]
    fn model_game_plan_keeps_a_merging_dag_with_exact_endings() {
        let game = json!({"success_ending_count":1,"failure_ending_count":1,"branch_min":2,"branch_max":2,"node_duration_min":5,"node_duration_max":10,"node_script_max_chars":40});
        let response = r#"{"assets":[],"nodes":[{"id":"start","node_type":"start","title":"起点","original_text":"主角收到消息。","prompt":"场景、角色、镜头、光线、承接、后果","duration_seconds":5},{"id":"left","node_type":"normal","title":"左路","original_text":"主角进入左巷。","prompt":"场景、角色、镜头、光线、承接、后果","duration_seconds":5},{"id":"right","node_type":"normal","title":"右路","original_text":"主角进入右巷。","prompt":"场景、角色、镜头、光线、承接、后果","duration_seconds":5},{"id":"merge","node_type":"normal","title":"汇合","original_text":"线索在钟楼汇合。","prompt":"场景、角色、镜头、光线、承接、后果","duration_seconds":5},{"id":"success","node_type":"success","title":"成功","original_text":"主角破解谜题。","prompt":"场景、角色、镜头、光线、承接、后果","duration_seconds":5},{"id":"failure","node_type":"failure","title":"失败","original_text":"主角错失机会。","prompt":"场景、角色、镜头、光线、承接、后果","duration_seconds":5}],"edges":[{"id":"e1","source_node_id":"start","target_node_id":"left","option_text":"检查左巷的血迹"},{"id":"e2","source_node_id":"start","target_node_id":"right","option_text":"追随右巷的脚印"},{"id":"e3","source_node_id":"left","target_node_id":"merge","option_text":"带着证据赶往钟楼"},{"id":"e4","source_node_id":"left","target_node_id":"failure","option_text":"冒险翻越封锁线"},{"id":"e5","source_node_id":"right","target_node_id":"merge","option_text":"循着钟声赶往钟楼"},{"id":"e6","source_node_id":"right","target_node_id":"failure","option_text":"向黑影暴露位置"},{"id":"e7","source_node_id":"merge","target_node_id":"success","option_text":"交出完整证据"},{"id":"e8","source_node_id":"merge","target_node_id":"failure","option_text":"独自销毁证据"}]}"#;
        let plan = model_game_plan(response, &game).expect("valid game graph");
        assert_eq!(plan["nodes"].as_array().expect("nodes").len(), 6);
        assert_eq!(plan["edges"].as_array().expect("edges").len(), 8);
        assert!(plan["nodes"][0]["prompt"]
            .as_str()
            .is_some_and(|prompt| prompt.contains("主角收到消息。")));
    }

    #[test]
    fn model_game_plan_accepts_early_endings_and_linear_dag_segments() {
        let game = json!({"success_ending_count":1,"failure_ending_count":2,"branch_min":2,"branch_max":2,"node_duration_min":5,"node_duration_max":10,"node_script_max_chars":40});
        let response = r#"{"assets":[],"nodes":[{"id":"A","node_type":"start","title":"起点","original_text":"收到线索。","prompt":"场景：起点","duration_seconds":5},{"id":"B","node_type":"normal","title":"追查左路","original_text":"进入左侧通道。","prompt":"场景：左路","duration_seconds":5},{"id":"C","node_type":"normal","title":"追查右路","original_text":"进入右侧通道。","prompt":"场景：右路","duration_seconds":5},{"id":"D","node_type":"normal","title":"确认真相","original_text":"证据完整。","prompt":"场景：钟楼","duration_seconds":5},{"id":"E","node_type":"normal","title":"暴露行踪","original_text":"敌人已经发现主角。","prompt":"场景：暗巷","duration_seconds":5},{"id":"F","node_type":"success","title":"成功","original_text":"主角带着证据脱身。","prompt":"场景：终局","duration_seconds":5},{"id":"G","node_type":"failure","title":"失败一","original_text":"主角错失机会。","prompt":"场景：终局","duration_seconds":5},{"id":"H","node_type":"failure","title":"失败二","original_text":"主角当场被困。","prompt":"场景：终局","duration_seconds":5}],"edges":[{"id":"AB","source_node_id":"A","target_node_id":"B","option_text":"检查左侧入口"},{"id":"AC","source_node_id":"A","target_node_id":"C","option_text":"追随右侧脚印"},{"id":"BD","source_node_id":"B","target_node_id":"D","option_text":"带着证据前往钟楼"},{"id":"BE","source_node_id":"B","target_node_id":"E","option_text":"冒险翻越围栏"},{"id":"CE","source_node_id":"C","target_node_id":"E","option_text":"误入监控盲区"},{"id":"CH","source_node_id":"C","target_node_id":"H","option_text":"直接闯入禁区"},{"id":"DF","source_node_id":"D","target_node_id":"F","option_text":"按既定路线撤离"},{"id":"EG","source_node_id":"E","target_node_id":"G","option_text":"暴露后果已经发生"}]}"#;
        let plan = model_game_plan(response, &game).expect("valid uneven DAG");
        let nodes = plan["nodes"].as_array().expect("nodes");
        let position = |id| {
            nodes.iter().find(|node| node["id"] == id).unwrap()["position_x"]
                .as_i64()
                .unwrap()
        };
        assert!(position("H") < position("F"));
    }

    #[test]
    fn fallback_game_plan_includes_an_early_failure_and_a_merge() {
        let game = json!({"script":"主角必须在废弃车站避开追捕并带走关键证据。","success_ending_count":1,"failure_ending_count":2,"branch_min":2,"branch_max":2,"node_duration_min":5,"node_duration_max":10,"node_script_max_chars":40});
        let plan = fallback_game_plan(&game);
        assert!(model_game_plan(&plan.to_string(), &game).is_some());
        let edges = plan["edges"].as_array().expect("edges");
        assert!(edges
            .iter()
            .any(|edge| edge["source_node_id"] == "start" && edge["target_node_id"] == "ending_2"));
        assert_eq!(
            edges
                .iter()
                .filter(|edge| edge["target_node_id"] == "merge")
                .count(),
            2
        );
        assert!(plan["nodes"].as_array().expect("nodes").iter().all(|node| {
            node["prompt"]
                .as_str()
                .is_some_and(|prompt| prompt.contains("原始剧情依据（必须画面化）："))
        }));
    }

    #[test]
    fn fallback_game_plan_delays_early_choice_state_until_a_later_decision() {
        let game = json!({"script":"主角必须在废弃车站避开追捕并带走关键证据。","success_ending_count":1,"failure_ending_count":30,"branch_min":2,"branch_max":4,"node_duration_min":5,"node_duration_max":10,"node_script_max_chars":40});
        let plan = fallback_game_plan(&game);
        let edges = plan["edges"].as_array().expect("edges");
        assert!(edges.iter().any(|edge| edge["source_node_id"] == "route_1"
            && edge["conditions"]["set"]["evidence_secured"].is_boolean()));
        assert!(edges
            .iter()
            .any(|edge| edge["source_node_id"] == "decision_4"
                && edge["conditions"]["requires"]["evidence_secured"].is_boolean()));
    }
}
