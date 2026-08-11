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
