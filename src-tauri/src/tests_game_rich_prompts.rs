//! Regression coverage for persisted interactive-game rich prompt reference nodes.

use std::fs;

use serde_json::{json, Map};

use crate::{db::Database, repository::Repository, value::new_id};

#[test]
fn game_node_keeps_creator_placed_reference_chips_after_reload() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("雾港抉择")),
            (
                "script".to_owned(),
                json!("玩家在雾港码头寻找失踪同伴，并在两条航线之间做出抉择。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    repository
        .save_game_graph(
            game_id,
            &[json!({"id":"dock","type":"scene","name":"雾港码头","prompt":"雨夜码头"})],
            &[json!({"id":"start","node_type":"start","title":"码头入口","original_text":"抵达码头","prompt":"场景：@图1（雾港码头）","duration_seconds":10})],
            &[],
        )
        .expect("save graph");
    let graph = repository.get_game(game_id).expect("load graph");
    let node_id = graph["nodes"][0]["id"].as_str().expect("node id");
    let asset_id = graph["assets"]
        .as_array()
        .expect("assets")
        .iter()
        .find(|asset| asset["name"] == "雾港码头")
        .and_then(|asset| asset["id"].as_str())
        .expect("scene id");
    let prompt_rich = json!([
        {"type":"text","text":"场景："},
        {"type":"reference","asset_id":asset_id,"asset_type":"scene","label":"雾港码头","image_url":null,"mention_number":1},
        {"type":"text","text":"，镜头缓慢推进。"}
    ]);

    repository
        .update_game_node(
            game_id,
            node_id,
            Map::from_iter([
                (
                    "prompt".to_owned(),
                    json!("场景：@图1（雾港码头），镜头缓慢推进。"),
                ),
                ("prompt_rich".to_owned(), prompt_rich.clone()),
                ("reference_asset_ids".to_owned(), json!([asset_id])),
            ]),
        )
        .expect("save rich prompt");

    let reloaded = repository
        .get_game_node(game_id, node_id)
        .expect("reload node");
    assert_eq!(reloaded["prompt_rich"], prompt_rich);
    assert_eq!(reloaded["reference_asset_ids"], json!([asset_id]));
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn generated_game_prompt_persists_the_selected_template_and_all_reference_dependencies() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("雾港抉择")),
            (
                "script".to_owned(),
                json!("玩家在雾港码头寻找失踪同伴，并在两条航线之间做出抉择。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    repository.save_game_graph(game_id,
        &[json!({"id":"dock","type":"scene","name":"雾港码头","prompt":"雨夜码头"}), json!({"id":"hero","type":"character","name":"林砚","prompt":"调查员"})],
        &[json!({"id":"start","node_type":"start","title":"码头入口","original_text":"林砚抵达雾港码头。","prompt":"旧提示词","duration_seconds":10})],
        &[],
    ).expect("save graph");
    let graph = repository.get_game(game_id).expect("load graph");
    let node_id = graph["nodes"][0]["id"].as_str().expect("node id");
    let dock_id = graph["assets"]
        .as_array()
        .expect("assets")
        .iter()
        .find(|asset| asset["name"] == "雾港码头")
        .and_then(|asset| asset["id"].as_str())
        .expect("dock id");
    let hero_id = graph["assets"]
        .as_array()
        .expect("assets")
        .iter()
        .find(|asset| asset["name"] == "林砚")
        .and_then(|asset| asset["id"].as_str())
        .expect("hero id");
    let nodes = vec![
        json!({"type":"text","text":"场景："}),
        json!({"type":"reference","asset_id":dock_id,"asset_type":"scene","label":"雾港码头"}),
        json!({"type":"text","text":"\n角色："}),
        json!({"type":"reference","asset_id":hero_id,"asset_type":"character","label":"林砚"}),
        json!({"type":"text","text":"\n风格：真人风格\n光线：雨夜\n位置：人物站在码头。\n【镜头1 | 时长10s | 时间：夜 外】镜头缓慢推进。"}),
    ];

    repository
        .save_generated_game_node_prompt(
            game_id,
            node_id,
            "场景：@图1（雾港码头）\n角色：@图2（林砚）",
            &nodes,
            "v2",
        )
        .expect("save generated prompt");
    let task = repository
        .enqueue_game_node_prompt(game_id, node_id)
        .expect("queue prompt");
    let node = repository
        .get_game_node(game_id, node_id)
        .expect("reload node");

    assert_eq!(node["prompt_template_version"], "v2");
    assert_eq!(node["reference_asset_ids"], json!([dock_id, hero_id]));
    assert_eq!(
        node["prompt_rich"]
            .as_array()
            .expect("rich nodes")
            .iter()
            .filter(|node| node["type"] == "reference")
            .count(),
        2
    );
    assert_eq!(task["type"], "game_node_prompt");
    assert_eq!(task["input_snapshot"]["prompt_template_version"], "v2");
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn game_node_rejects_a_reference_asset_from_another_game() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let create = |name| {
        repository
            .create_game(Map::from_iter([
                ("name".to_owned(), json!(name)),
                (
                    "script".to_owned(),
                    json!("玩家在雨夜码头选择进入仓库或追赶远处的船只，并承担不同后果。"),
                ),
            ]))
            .expect("create game")
    };
    let first = create("游戏一");
    let second = create("游戏二");
    let first_id = first["id"].as_str().expect("first game");
    let second_id = second["id"].as_str().expect("second game");
    for (game_id, asset) in [(first_id, "本游戏场景"), (second_id, "另一个游戏场景")] {
        repository.save_game_graph(game_id, &[json!({"id":"scene","type":"scene","name":asset,"prompt":"码头"})], &[json!({"id":"start","node_type":"start","title":"入口","original_text":"抵达码头","prompt":"提示词","duration_seconds":10})], &[]).expect("save graph");
    }
    let first_node = repository.get_game(first_id).expect("first graph")["nodes"][0]["id"]
        .as_str()
        .expect("first node")
        .to_owned();
    let foreign_asset = repository.get_game(second_id).expect("second graph")["assets"][0]["id"]
        .as_str()
        .expect("foreign asset")
        .to_owned();

    let result = repository.update_game_node(
        first_id,
        &first_node,
        Map::from_iter([("reference_asset_ids".to_owned(), json!([foreign_asset]))]),
    );

    assert!(result.is_err());
    fs::remove_dir_all(root).expect("remove test data");
}
