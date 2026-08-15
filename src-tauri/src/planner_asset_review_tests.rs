use serde_json::json;

use super::{model_game_plan, review_assets, AssetEvidence};

const WATERMELON_SCRIPT: &str = "【剧情段 S01｜开始】\n场景：西瓜村村口晒谷场，正午。\n出场角色与道具：玩家（林小满，背帆布包）、村正林德顺（玩家爷爷，70岁，白汗衫，攥铜烟袋）、外乡人张磊（黑框眼镜，脚边放竹筐）、围观村民（王二婶抱孩子、王老汉叼烟）、铜烟袋、竹筐、蜜瓜。\n剧情正文：林小满走到林德顺身边，张磊递上蜜瓜。";

#[test]
fn reviewer_extracts_named_roles_from_combined_cast_and_prop_manifests() {
    let evidence = AssetEvidence::from_script(WATERMELON_SCRIPT);
    for name in ["林小满", "林德顺", "张磊", "王二婶", "王老汉"] {
        assert!(
            evidence.names("character").contains(&name.to_owned()),
            "missing {name}"
        );
    }
    assert!(!evidence.names("character").contains(&"白汗衫".to_owned()));
}

#[test]
fn drama_reviewer_discards_compound_prop_and_adds_missing_named_character() {
    let assets = review_assets(
        WATERMELON_SCRIPT,
        "乡村",
        vec![
            json!({"type":"character","name":"张磊","prompt":"外乡农技人员。"}),
            json!({"type":"prop","name":"你、爷爷林德顺（叼烟袋走过来）、蜜瓜样本","prompt":"错误的混合清单。"}),
        ],
    );

    assert!(assets
        .iter()
        .any(|asset| asset["type"] == "character" && asset["name"] == "林德顺"));
    assert!(assets.iter().all(|asset| {
        asset["name"]
            .as_str()
            .is_none_or(|name| !name.contains('、'))
    }));
}

#[test]
fn reviewer_does_not_treat_leather_bag_or_notebook_as_people() {
    let script = "角色：王德福\n王德福拎着皮公文包，拿出皮笔记本记录。";
    let assets = review_assets(
        script,
        "年代剧",
        vec![
            json!({"type":"character","name":"皮公文包","source_evidence":"王德福拎着皮公文包","prompt":"皮质公文包"}),
            json!({"type":"character","name":"皮笔记本","source_evidence":"拿出皮笔记本记录","prompt":"皮质笔记本"}),
        ],
    );
    assert!(!assets.iter().any(|asset| asset["type"] == "character"
        && ["皮公文包", "皮笔记本"].contains(&asset["name"].as_str().unwrap_or_default())));
    assert!(assets
        .iter()
        .any(|asset| asset["type"] == "prop" && asset["name"] == "皮公文包"));
    assert!(assets
        .iter()
        .any(|asset| asset["type"] == "prop" && asset["name"] == "皮笔记本"));
}

#[test]
fn reviewer_removes_name_with_stuck_quantifier_tail() {
    let script = "角色：王德福\n王德福一把拿起皮公文包。";
    let assets = review_assets(
        script,
        "年代剧",
        vec![
            json!({"type":"character","name":"王德福","source_evidence":"王德福一把拿起","prompt":"村长"}),
            json!({"type":"character","name":"王德福一","source_evidence":"王德福一把拿起","prompt":"村长"}),
        ],
    );
    assert!(assets
        .iter()
        .any(|asset| asset["type"] == "character" && asset["name"] == "王德福"));
    assert!(!assets
        .iter()
        .any(|asset| asset["type"] == "character" && asset["name"] == "王德福一"));
}

#[test]
fn game_reviewer_repairs_model_materials_before_graph_persistence() {
    let game = json!({
        "script": WATERMELON_SCRIPT,
        "expanded_script": WATERMELON_SCRIPT,
        "success_ending_count": 1,
        "failure_ending_count": 1,
        "branch_min": 2,
        "branch_max": 2,
        "node_duration_min": 5,
        "node_duration_max": 10,
        "node_script_max_chars": 40,
    });
    let response = r#"{
        "assets":[
            {"id":"zhang","type":"character","name":"张磊","prompt":"外乡农技人员。"},
            {"id":"bad","type":"prop","name":"你、爷爷林德顺（叼烟袋走过来）、蜜瓜样本","prompt":"错误的混合清单。"}
        ],
        "nodes":[
            {"id":"start","node_type":"start","title":"起点","original_text":"林小满回村。","prompt":"场景：晒谷场","duration_seconds":5},
            {"id":"left","node_type":"normal","title":"左路","original_text":"林德顺劝阻。","prompt":"场景：晒谷场","reference_asset_ids":["村正林德顺"],"duration_seconds":5},
            {"id":"right","node_type":"normal","title":"右路","original_text":"张磊展示蜜瓜。","prompt":"场景：晒谷场","duration_seconds":5},
            {"id":"success","node_type":"success","title":"成功","original_text":"村民接受检测。","prompt":"场景：晒谷场","duration_seconds":5},
            {"id":"failure","node_type":"failure","title":"失败","original_text":"冲突失控。","prompt":"场景：晒谷场","duration_seconds":5}
        ],
        "edges":[
            {"id":"a","source_node_id":"start","target_node_id":"left","option_text":"先安抚爷爷"},
            {"id":"b","source_node_id":"start","target_node_id":"right","option_text":"查看蜜瓜"},
            {"id":"c","source_node_id":"left","target_node_id":"success","option_text":"说明检测结论"},
            {"id":"d","source_node_id":"left","target_node_id":"failure","option_text":"坚持旧规"},
            {"id":"e","source_node_id":"right","target_node_id":"success","option_text":"展示检测报告"},
            {"id":"f","source_node_id":"right","target_node_id":"failure","option_text":"质疑外乡人"}
        ]
    }"#;

    let plan = model_game_plan(response, &game).expect("valid graph");
    assert!(plan["assets"].as_array().is_some_and(|assets| {
        assets
            .iter()
            .any(|asset| asset["type"] == "character" && asset["name"] == "林德顺")
    }));
    assert!(plan["assets"].as_array().is_some_and(|assets| {
        assets.iter().all(|asset| {
            asset["name"]
                .as_str()
                .is_none_or(|name| !name.contains('、'))
        })
    }));
    assert_eq!(
        plan["nodes"][1]["reference_asset_ids"],
        json!(["character:林德顺"])
    );
    let assets = plan["assets"].as_array().expect("assets");
    for (kind, labels) in [
        (
            "character",
            ["角色身份与性格：", "外观设定：", "连续性要求："],
        ),
        (
            "scene",
            ["场景名称与剧情用途：", "空间与主体：", "陈设与氛围："],
        ),
        ("prop", ["道具名称与叙事用途：", "外观细节：", "呈现限制："]),
    ] {
        let prompt = assets
            .iter()
            .find(|asset| asset["type"] == kind)
            .and_then(|asset| asset["prompt"].as_str())
            .expect("formatted material prompt");
        assert!(prompt.starts_with("叙述背景主题：互动游戏\n风格：真人风格\n"));
        assert!(
            labels.into_iter().all(|label| prompt.contains(label)),
            "missing {kind} prompt section"
        );
    }
}
