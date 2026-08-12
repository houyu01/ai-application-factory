use serde_json::json;

use super::{
    game_graph_progress_checkpoint, game_graph_stage, game_graph_stage_prompt,
    merge_game_graph_resume, merge_game_graph_stage_response, model_game_plan,
    resume_game_graph_prompt, GameGraphStage,
};

fn game() -> serde_json::Value {
    json!({
        "success_ending_count": 1,
        "failure_ending_count": 1,
        "branch_min": 2,
        "branch_max": 2,
        "node_duration_min": 5,
        "node_duration_max": 10,
        "node_script_max_chars": 400,
        "style": "真人风格",
        "resolution": "720p",
    })
}

fn graph_with_bad_middle_node() -> serde_json::Value {
    json!({
        "assets": [],
        "nodes": [
            {"id":"start","node_type":"start","title":"钟楼入口","original_text":"调查员推开钟楼的木门，录音机在黑暗里自动播放。","prompt":"钟楼入口里的调查员和旧录音机。","duration_seconds":5},
            {"id":"middle","node_type":"unexpected","title":"旋梯线索","original_text":"调查员沿旋梯发现红灯正对着失踪搭档的暗号。","prompt":"钟楼旋梯上的红灯与暗号。","duration_seconds":5},
            {"id":"success","node_type":"success","title":"公开真相","original_text":"调查员破解暗号后在钟顶公开录音，失踪搭档得救。","prompt":"钟顶公开录音并迎来清晨。","duration_seconds":5},
            {"id":"failure","node_type":"failure","title":"警报吞没","original_text":"调查员错误触发警报，钟楼的闸门把证据永远锁住。","prompt":"警报灯下落下的钟楼闸门。","duration_seconds":5}
        ],
        "edges": [
            {"id":"e1","source_node_id":"start","target_node_id":"middle","option_text":"循着录音机的回声登上旋梯","sort_order":1},
            {"id":"e2","source_node_id":"start","target_node_id":"failure","option_text":"砸碎录音机以阻断未知信号","sort_order":2},
            {"id":"e3","source_node_id":"middle","target_node_id":"success","option_text":"按暗号节奏敲响钟绳","sort_order":1}
        ]
    })
}

#[test]
fn graph_checkpoint_keeps_nodes_and_branches_around_a_bad_middle_node() {
    let response = graph_with_bad_middle_node().to_string();
    let checkpoint = game_graph_progress_checkpoint(&response, None);

    let saved_nodes = checkpoint["nodes"].as_array().expect("saved nodes");
    assert_eq!(saved_nodes.len(), 3);
    assert!(saved_nodes.iter().all(|node| node["id"] != "middle"));
    assert_eq!(checkpoint["edges"].as_array().map(Vec::len), Some(3));
    assert!(resume_game_graph_prompt(&checkpoint).contains("已保存断点"));
    assert!(resume_game_graph_prompt(&checkpoint).contains("钟楼入口"));
}

#[test]
fn repaired_middle_node_resumes_the_saved_graph_without_replacing_its_prefix() {
    let response = graph_with_bad_middle_node().to_string();
    let checkpoint = game_graph_progress_checkpoint(&response, None);
    let repair = json!({
        "assets": [],
        "nodes": [
            {"id":"middle","node_type":"normal","title":"旋梯线索","original_text":"调查员沿旋梯发现红灯正对着失踪搭档的暗号。","prompt":"钟楼旋梯上的红灯与暗号。","duration_seconds":5}
        ],
        "edges": []
    })
    .to_string();

    let merged = merge_game_graph_resume(&checkpoint, &repair).expect("merged graph");
    assert_eq!(merged["nodes"].as_array().map(Vec::len), Some(4));
    assert_eq!(merged["edges"].as_array().map(Vec::len), Some(3));
    assert_eq!(merged["nodes"][0]["id"], "start");
    assert!(model_game_plan(&merged.to_string(), &game()).is_some());
}

fn staged_checkpoint() -> serde_json::Value {
    json!({
        "assets": [
            {"id":"lin_mo","type":"character","name":"林默","prompt":"雨夜追查钟楼录音的调查员。"}
        ],
        "nodes": [
            {"id":"start","node_type":"start","title":"钟楼入口","original_text":"林默推开钟楼木门，录音机在黑暗里播放失踪搭档的暗号。","prompt":"钟楼入口里的林默和旧录音机。","reference_asset_ids":["lin_mo"],"duration_seconds":5},
            {"id":"success","node_type":"success","title":"公开真相","original_text":"林默带着录音登上钟顶公开真相，失踪搭档在晨光中获救。","prompt":"钟顶晨光下公开录音的林默。","reference_asset_ids":["lin_mo"],"duration_seconds":5},
            {"id":"failure","node_type":"failure","title":"警报吞没","original_text":"林默砸碎录音机后触发警报，钟楼闸门将证据永久锁住。","prompt":"警报灯下被闸门困住的林默。","reference_asset_ids":["lin_mo"],"duration_seconds":5}
        ],
        "edges": []
    })
}

#[test]
fn completed_assets_advance_to_the_node_stage_without_regenerating_them() {
    let checkpoint = json!({"assets": staged_checkpoint()["assets"], "nodes": [], "edges": []});

    assert_eq!(
        game_graph_stage(&checkpoint, &game()),
        GameGraphStage::Nodes
    );
    let prompt = game_graph_stage_prompt(
        GameGraphStage::Nodes,
        &game(),
        "钟楼互动剧本",
        &checkpoint,
        None,
    );
    assert!(prompt.contains("【本次输出阶段：节点】"));
    assert!(prompt.contains("已保存素材目录"));
    assert!(!prompt.contains("\"assets\":[{"));
}

#[test]
fn valid_node_checkpoint_needs_only_edges_before_final_dag_validation() {
    let checkpoint = staged_checkpoint();
    let edges = json!({
        "edges": [
            {"id":"start-success","source_node_id":"start","target_node_id":"success","option_text":"带着录音登上钟顶公开真相","sort_order":1},
            {"id":"start-failure","source_node_id":"start","target_node_id":"failure","option_text":"砸碎录音机阻断未知信号","sort_order":2}
        ]
    })
    .to_string();

    assert_eq!(
        game_graph_stage(&checkpoint, &game()),
        GameGraphStage::Edges
    );
    let merged = merge_game_graph_stage_response(GameGraphStage::Edges, &edges, &checkpoint)
        .expect("edge response should merge");
    assert_eq!(merged["assets"], checkpoint["assets"]);
    assert_eq!(merged["nodes"], checkpoint["nodes"]);
    assert_eq!(merged["edges"].as_array().map(Vec::len), Some(2));
    assert!(model_game_plan(&merged.to_string(), &game()).is_some());
}
