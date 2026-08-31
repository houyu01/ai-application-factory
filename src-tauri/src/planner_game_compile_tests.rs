use serde_json::json;

use super::super::{fallback_game_expansion, playable_game_plan};
use super::compile_game_plan;

fn game() -> serde_json::Value {
    json!({
        "success_ending_count": 2,
        "failure_ending_count": 12,
        "branch_min": 2,
        "branch_max": 2,
        "node_duration_min": 5,
        "node_duration_max": 10,
        "node_script_max_chars": 400,
        "style": "真人风格",
        "resolution": "720p",
        "script": "调查员在钟楼追查失踪搭档。",
    })
}

#[test]
fn compiles_a_playable_dag_from_a_structured_screenplay() {
    let screenplay = "【剧情段 S01｜开始】\n场景：钟楼入口\n出场角色与道具：调查员、录音机\n剧情正文：调查员推开木门，录音机自动播放。\n【玩家抉择】\n- 选择：循着录音登上旋梯\n  前往：S02\n- 选择：砸碎录音机\n  前往：E02\n【剧情段 S02｜旋梯】\n剧情正文：调查员沿旋梯发现红灯暗号。\n【玩家抉择】\n- 选择：按暗号敲钟绳\n  前往：E01\n【结局 E01｜成功】\n结局正文：调查员公开录音，搭档得救。\n【结局 E02｜失败】\n结局正文：警报吞没钟楼。";
    let mut game = game();
    game["expanded_script"] = json!(screenplay);

    let plan = compile_game_plan(screenplay, &game).expect("compiled");
    let nodes = plan["nodes"].as_array().expect("nodes");
    let edges = plan["edges"].as_array().expect("edges");
    assert!(nodes
        .iter()
        .any(|node| node["id"] == "s01" && node["node_type"] == "start"));
    assert!(nodes.iter().any(|node| node["node_type"] == "success"));
    assert!(nodes.iter().any(|node| node["node_type"] == "failure"));
    assert!(edges
        .iter()
        .any(|edge| edge["source_node_id"] == "s01" && edge["target_node_id"] == "s02"));
    assert!(playable_game_plan(&plan.to_string(), &game).is_some());
    assert!(plan["assets"]
        .as_array()
        .is_some_and(|assets| !assets.is_empty()));
}

#[test]
fn repairs_missing_choices_instead_of_emitting_an_unplayable_dag() {
    let screenplay = "【剧情段 S01｜开始】\n剧情正文：钟楼警报响起。\n【玩家抉择】\n【结局 E01｜成功】\n结局正文：真相公开。\n【结局 E02｜失败】\n结局正文：闸门落下。";
    let mut game = game();
    game["expanded_script"] = json!(screenplay);

    let plan = compile_game_plan(screenplay, &game).expect("repaired");
    let start_outs = plan["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .filter(|edge| edge["source_node_id"] == "s01")
        .count();
    assert!(start_outs >= 1);
    assert!(playable_game_plan(&plan.to_string(), &game).is_some());
}

#[test]
fn drops_a_cyclic_extra_choice_and_keeps_the_playable_tree() {
    let screenplay = "【剧情段 S01｜开始】\n剧情正文：入口。\n【玩家抉择】\n- 选择：进入旋梯\n  前往：S02\n- 选择：立刻失败\n  前往：E02\n【剧情段 S02｜旋梯】\n剧情正文：旋梯。\n【玩家抉择】\n- 选择：公开真相\n  前往：E01\n- 选择：返回入口\n  前往：S01\n【结局 E01｜成功】\n结局正文：成功。\n【结局 E02｜失败】\n结局正文：失败。";
    let mut game = game();
    game["expanded_script"] = json!(screenplay);

    let plan = compile_game_plan(screenplay, &game).expect("acyclic");
    let edges = plan["edges"].as_array().expect("edges");
    assert!(edges
        .iter()
        .all(|edge| !(edge["source_node_id"] == "s02" && edge["target_node_id"] == "s01")));
    assert!(playable_game_plan(&plan.to_string(), &game).is_some());
}

#[test]
fn compiles_offline_expansion_without_matching_configured_ending_counts() {
    let screenplay = fallback_game_expansion(&game());
    let mut game = game();
    game["expanded_script"] = json!(screenplay);
    let plan = compile_game_plan(&screenplay, &game).expect("compiled expansion");
    assert!(playable_game_plan(&plan.to_string(), &game).is_some());
}

#[test]
fn unstructured_prose_is_not_compiled() {
    assert!(compile_game_plan("调查员走进钟楼。", &game()).is_none());
}

#[test]
fn attaches_a_failure_when_the_screenplay_only_routes_to_success() {
    let screenplay = "【剧情段 S01｜开始】\n场景：钟楼入口\n出场角色与道具：调查员、录音机\n剧情正文：调查员推开木门。\n【玩家抉择】\n- 选择：公开录音\n  前往：E01\n- 选择：继续上楼\n  前往：E01\n【结局 E01｜成功】\n结局正文：真相公开。\n【结局 E02｜失败】\n结局正文：闸门落下。";
    let mut game = game();
    game["expanded_script"] = json!(screenplay);
    let plan = compile_game_plan(screenplay, &game).expect("repaired both endings");
    let edges = plan["edges"].as_array().expect("edges");
    assert!(edges.iter().any(|edge| edge["target_node_id"] == "e01"));
    assert!(edges.iter().any(|edge| edge["target_node_id"] == "e02"));
    assert!(playable_game_plan(&plan.to_string(), &game).is_some());
}

#[test]
fn compiled_nodes_keep_shootable_copy_and_reusable_assets() {
    let screenplay = "【剧情段 S01｜开始】\n场景：钟楼入口\n出场角色与道具：调查员、录音机\n剧情正文：调查员推开木门，录音机自动播放。\n【玩家抉择】\n- 选择：循着录音登上旋梯\n  前往：E01\n- 选择：砸碎录音机\n  前往：E02\n【结局 E01｜成功】\n结局正文：调查员公开录音。\n【结局 E02｜失败】\n结局正文：警报吞没钟楼。";
    let mut game = game();
    game["expanded_script"] = json!(screenplay);
    let plan = compile_game_plan(screenplay, &game).expect("compiled");
    let start = plan["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["id"] == "s01")
        .expect("start");
    let original = start["original_text"].as_str().expect("original");
    assert!(original.contains("录音机"));
    assert!(start["prompt"]
        .as_str()
        .is_some_and(|prompt| prompt.contains(original) || prompt.contains("录音机")));
    assert!(plan["assets"].as_array().is_some_and(|assets| {
        assets.iter().any(|asset| asset["type"] == "character")
            && assets.iter().any(|asset| asset["type"] == "scene")
    }));
}
