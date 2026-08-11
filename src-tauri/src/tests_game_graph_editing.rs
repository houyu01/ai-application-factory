//! Regression coverage for manually repositioned and relinked game DAGs.

use std::fs;

use serde_json::{json, Map};

use crate::{db::Database, repository::Repository, value::new_id};

#[test]
fn editing_a_choice_endpoint_rejects_a_cycle_and_keeps_the_old_edge() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("手动连线")),
            (
                "script".to_owned(),
                json!("玩家在钟楼的三段录像之间选择线索，并避开不断靠近的危险。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    repository
        .save_game_graph(
            game_id,
            &[],
            &[
                json!({"id":"a","node_type":"start","title":"入口","original_text":"进入钟楼。","prompt":"钟楼入口","duration_seconds":5}),
                json!({"id":"b","node_type":"normal","title":"回廊","original_text":"查看回廊。","prompt":"钟楼回廊","duration_seconds":5}),
                json!({"id":"c","node_type":"success","title":"出口","original_text":"安全离开。","prompt":"钟楼出口","duration_seconds":5}),
            ],
            &[
                json!({"id":"ab","source_node_id":"a","target_node_id":"b","option_text":"进入回廊","sort_order":1}),
                json!({"id":"bc","source_node_id":"b","target_node_id":"c","option_text":"前往出口","sort_order":1}),
            ],
        )
        .expect("save game graph");
    let graph = repository.get_game(game_id).expect("load graph");
    let edge = graph["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .find(|edge| edge["option_text"] == "进入回廊")
        .expect("edge")
        .clone();
    let nodes = graph["nodes"].as_array().expect("nodes");
    let back = nodes
        .iter()
        .find(|node| node["title"] == "回廊")
        .expect("back node")["id"]
        .clone();
    let later = nodes
        .iter()
        .find(|node| node["title"] == "出口")
        .expect("later node")["id"]
        .clone();

    let error = repository
        .update_game_edge(
            game_id,
            edge["id"].as_str().expect("edge id"),
            Map::from_iter([
                ("source_node_id".to_owned(), later),
                ("target_node_id".to_owned(), back),
            ]),
        )
        .expect_err("cycle must be rejected");
    assert!(error.to_string().contains("循环"));
    assert_eq!(
        repository.get_game(game_id).expect("reload graph")["edges"][0]["source_node_id"],
        edge["source_node_id"]
    );
    fs::remove_dir_all(root).expect("remove test data");
}
