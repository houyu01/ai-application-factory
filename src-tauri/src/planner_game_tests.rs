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
        "举手”“不举手",
        "和平谈判，通过外交解决争端",
        "必须依据来源与目标视频改写 option_text",
        "来源节点 original_text → option_text → 目标节点 original_text",
        "素材图片提示词格式（最高优先级）",
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
    let node = |id| nodes.iter().find(|node| node["id"] == id).cloned().unwrap();
    let coordinate = |id| {
        (
            node(id)["position_x"].as_i64().unwrap(),
            node(id)["position_y"].as_i64().unwrap(),
        )
    };
    assert!(coordinate("H").0 < coordinate("F").0);
    assert_eq!((coordinate("B").0, coordinate("C").1), (440, 270));
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
