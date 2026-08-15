//! Small, schema-shaped prompts for each durable interactive-game graph stage.

use std::collections::HashSet;

use serde_json::{json, Value};

use super::materials::GAME_ASSET_PROMPT_CONTRACT;
use super::{game_graph_progress_checkpoint, integer, parse_json_object};

const NODE_BATCH_LIMIT: usize = 4;

/// A single model call owns one graph record family so a long material list cannot truncate nodes or edges.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GameGraphStage {
    Assets,
    Nodes,
    Edges,
}

impl GameGraphStage {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Assets => "assets",
            Self::Nodes => "nodes",
            Self::Edges => "edges",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Assets => "角色、场景和道具",
            Self::Nodes => "视频节点骨架",
            Self::Edges => "玩家选择边",
        }
    }
}

/// Select the earliest incomplete stage, including checkpoints created by older all-in-one requests.
pub(crate) fn game_graph_stage(checkpoint: &Value, game: &Value) -> GameGraphStage {
    if checkpoint["assets"].as_array().is_none_or(Vec::is_empty) {
        GameGraphStage::Assets
    } else if !has_required_node_kinds(checkpoint["nodes"].as_array(), game) {
        GameGraphStage::Nodes
    } else {
        GameGraphStage::Edges
    }
}

/// Retain only the current stage's independently valid streamed records; edges wait for full-DAG validation.
pub(crate) fn game_graph_stage_checkpoint(
    stage: GameGraphStage,
    response: &str,
    existing: &Value,
) -> Value {
    let accepted = game_graph_progress_checkpoint(response, Some(existing));
    let mut checkpoint = json!({
        "assets": existing["assets"].as_array().cloned().unwrap_or_default(),
        "nodes": existing["nodes"].as_array().cloned().unwrap_or_default(),
        "edges": existing["edges"].as_array().cloned().unwrap_or_default(),
    });
    if matches!(stage, GameGraphStage::Assets | GameGraphStage::Nodes) {
        checkpoint[stage.key()] = accepted[stage.key()].clone();
    }
    checkpoint
}

/// Read a completed one-family JSON response and merge it into the durable graph checkpoint.
pub(crate) fn merge_game_graph_stage_response(
    stage: GameGraphStage,
    response: &str,
    existing: &Value,
) -> Option<Value> {
    let parsed = parse_json_object(response)?;
    let records = parsed[stage.key()].as_array()?;
    let mut checkpoint = game_graph_stage_checkpoint(stage, response, existing);
    if stage == GameGraphStage::Edges {
        checkpoint["edges"] = json!(records);
    }
    Some(checkpoint)
}

/// Re-serialize only durable, complete records after a streamed stage response stops mid-JSON.
///
/// The graph worker stores this closed object before asking the model for the next batch, so an
/// unfinished string or object is never treated as continuation context.
pub(crate) fn game_graph_stage_response(stage: GameGraphStage, checkpoint: &Value) -> String {
    json!({stage.key(): checkpoint[stage.key()]}).to_string()
}

/// Describe the narrow JSON response expected for the next model call and include only relevant context.
pub(crate) fn game_graph_stage_prompt(
    stage: GameGraphStage,
    game: &Value,
    screenplay: &str,
    checkpoint: &Value,
    feedback: Option<&str>,
) -> String {
    let branch_min = integer(game, "branch_min", 2, 2, 4);
    let branch_max = integer(game, "branch_max", 4, branch_min, 4);
    let duration_min = integer(game, "node_duration_min", 5, 3, 15);
    let duration_max = integer(game, "node_duration_max", 10, duration_min, 15);
    let feedback = feedback
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("\n\n上一次校验反馈（必须修复）：{value}"))
        .unwrap_or_default();
    let common = "只能返回一个 JSON 对象，不要 Markdown、解释、代码块或任何额外字段。严格只输出本阶段的一个数组字段；不要输出其他两个字段。";
    match stage {
        GameGraphStage::Assets => format!(
            "【本次输出阶段：素材】只生成可复用的角色、场景、道具，不生成节点或选择边。输出 schema：{{\"assets\":[{{\"id\":\"稳定唯一标识\",\"type\":\"character|scene|prop\",\"name\":\"名称\",\"prompt\":\"视觉提示词\"}}]}}。同名素材只保留一项，必须包含主人公角色。{common}\n\n{GAME_ASSET_PROMPT_CONTRACT}\n\n互动剧本：\n{screenplay}{feedback}",
        ),
        GameGraphStage::Nodes => format!(
            "【本次输出阶段：节点】只生成视频节点，不生成素材或选择边。输出 schema：{{\"nodes\":[{{\"id\":\"稳定唯一标识\",\"node_type\":\"start|normal|success|failure\",\"title\":\"标题\",\"original_text\":\"本节点剧情正文\",\"prompt\":\"视频提示词\",\"reference_asset_ids\":[\"已给素材 id\"],\"duration_seconds\":{duration_min}}}]}}。合并已保存节点后，必须恰好有 1 个 start、{} 个 success、{} 个 failure；每个节点的 original_text 与 prompt 均不可重复，时长为 {duration_min}-{duration_max} 秒。素材只能引用下方目录中的 id。已保存节点不可改写，只补缺失节点。{}{common}\n\n已保存素材目录：\n{}\n\n已保存节点：\n{}\n\n互动剧本：\n{screenplay}{feedback}",
            integer(game, "success_ending_count", 2, 1, 100),
            integer(game, "failure_ending_count", 12, 1, 200),
            node_batch_instruction(checkpoint, game),
            compact_assets(checkpoint),
            compact_nodes(checkpoint),
        ),
        GameGraphStage::Edges => format!(
            "【本次输出阶段：选择边】只生成选择边，不生成素材或节点。输出 schema：{{\"edges\":[{{\"id\":\"稳定唯一标识\",\"source_node_id\":\"已给节点 id\",\"target_node_id\":\"已给节点 id\",\"option_text\":\"玩家可点击的具体选择\",\"sort_order\":1,\"conditions\":{{\"requires\":{{\"snake_case\":true}},\"set\":{{\"snake_case\":true}}}}}}]}}。start 的出边数必须为 {branch_min}-{branch_max}；normal 节点只能有 1 条承接边或 {branch_min}-{branch_max} 条互斥选择边；success/failure 没有出边。边必须构成从 start 可达、无环、每个非结局可到达结局的 DAG。只使用下方节点 id，option_text 必须连接来源节点结尾与目标节点开场，不能使用“继续”“选项 A”等泛化文案。{common}\n\n节点目录：\n{}\n\n互动剧本：\n{screenplay}{feedback}",
            compact_nodes(checkpoint),
        ),
    }
}

fn node_batch_instruction(checkpoint: &Value, game: &Value) -> String {
    let count = |kind: &str| {
        checkpoint["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|node| node["node_type"] == kind)
            .count()
    };
    let start = count("start");
    let normal = count("normal");
    let success = count("success");
    let failure = count("failure");
    let target_success = integer(game, "success_ending_count", 2, 1, 100) as usize;
    let target_failure = integer(game, "failure_ending_count", 12, 1, 200) as usize;
    let maximum = integer(game, "branch_max", 4, 2, 4) as usize;
    let endings = target_success + target_failure;
    let minimum_internal = endings.saturating_sub(1).div_ceil(maximum - 1);
    let minimum_normal = minimum_internal.saturating_sub(1);
    format!(
        "\n\n【断点续生成，必须遵守】已保存 start {start}/1、normal {normal}/至少 {minimum_normal}、success {success}/{target_success}、failure {failure}/{target_failure}。本次 nodes 数组只能新增 1-{NODE_BATCH_LIMIT} 个节点，必须是完整闭合的 JSON；绝不能重发已保存 id，也不能输出半个字符串、半个对象或未闭合数组。为使最多 {maximum} 条出边能够覆盖全部 {endings} 个结局，normal 总数至少需要 {minimum_normal} 个；补齐结局时也要继续补足可承接它们的 normal 节点。"
    )
}

/// Return a focused error for the next edge-only request without leaking a rejected response into persistence.
pub(crate) fn game_graph_edge_feedback(checkpoint: &Value, response: &str, game: &Value) -> String {
    let Some(parsed) = parse_json_object(response) else {
        return "选择边阶段没有返回完整 JSON 对象；只返回 edges 数组。".to_owned();
    };
    let Some(edges) = parsed["edges"].as_array() else {
        return "选择边阶段缺少 edges 数组。".to_owned();
    };
    let ids = checkpoint["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| node["id"].as_str())
        .collect::<HashSet<_>>();
    let invalid = edges.iter().find(|edge| {
        let source = edge["source_node_id"].as_str().unwrap_or_default();
        let target = edge["target_node_id"].as_str().unwrap_or_default();
        source.is_empty()
            || target.is_empty()
            || source == target
            || !ids.contains(source)
            || !ids.contains(target)
    });
    if let Some(edge) = invalid {
        return format!("边 {} 引用了不存在或相同的节点，请只使用节点目录中的不同 source_node_id 与 target_node_id。", edge["id"].as_str().unwrap_or("（未命名）"));
    }
    let mut outgoing = std::collections::HashMap::<&str, usize>::new();
    for edge in edges {
        *outgoing
            .entry(edge["source_node_id"].as_str().unwrap_or_default())
            .or_default() += 1;
    }
    let minimum = integer(game, "branch_min", 2, 2, 4) as usize;
    let maximum = integer(game, "branch_max", 4, minimum as i64, 4) as usize;
    if let Some(node) = checkpoint["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|node| node["node_type"] == "start")
    {
        let id = node["id"].as_str().unwrap_or_default();
        let count = outgoing.get(id).copied().unwrap_or_default();
        if !(minimum..=maximum).contains(&count) {
            return format!(
                "start 节点 {id} 需要 {minimum}-{maximum} 条选择边，当前为 {count} 条。"
            );
        }
    }
    "选择边未通过完整 DAG 校验：检查重复选项、不可达节点、无结局路径或环路，并重新返回完整 edges 数组。".to_owned()
}

fn has_required_node_kinds(nodes: Option<&Vec<Value>>, game: &Value) -> bool {
    let Some(nodes) = nodes else {
        return false;
    };
    let mut ids = HashSet::new();
    let count = |kind| {
        nodes
            .iter()
            .filter(|node| node["node_type"] == kind)
            .count() as i64
    };
    !nodes.is_empty()
        && nodes.iter().all(|node| {
            node["id"]
                .as_str()
                .is_some_and(|id| ids.insert(id.to_owned()))
        })
        && count("start") == 1
        && count("success") == integer(game, "success_ending_count", 2, 1, 100)
        && count("failure") == integer(game, "failure_ending_count", 12, 1, 200)
}

fn compact_assets(checkpoint: &Value) -> Value {
    json!(checkpoint["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|asset| json!({"id":asset["id"],"type":asset["type"],"name":asset["name"]}))
        .collect::<Vec<_>>())
}

fn compact_nodes(checkpoint: &Value) -> Value {
    json!(checkpoint["nodes"].as_array().into_iter().flatten().map(|node| json!({"id":node["id"],"node_type":node["node_type"],"title":node["title"],"original_text":node["original_text"]})).collect::<Vec<_>>())
}
