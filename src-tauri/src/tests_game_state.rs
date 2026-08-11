//! Regression coverage for deferred choice effects that survive a shared-video DAG merge.

use std::fs;

use serde_json::{json, Map};

use crate::{db::Database, repository::Repository, value::new_id};

#[test]
fn early_choice_state_selects_a_later_success_or_failure_after_a_merge() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("状态回响")),
            ("script".to_owned(), json!("玩家先决定是否保留证据，随后两条路径会汇合到同一座钟楼，最后的抉择取决于此前的决定。")),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    repository
        .save_game_graph(
            game_id,
            &[],
            &[
                node("start", "start"),
                node("shared", "normal"),
                node("success", "success"),
                node("failure", "failure"),
            ],
            &[
                edge(
                    "keep",
                    "start",
                    "shared",
                    "保留证据",
                    json!({"set":{"evidence_secured":true}}),
                ),
                edge(
                    "discard",
                    "start",
                    "shared",
                    "丢弃证据",
                    json!({"set":{"evidence_secured":false}}),
                ),
                edge(
                    "win",
                    "shared",
                    "success",
                    "交出证据",
                    json!({"requires":{"evidence_secured":true}}),
                ),
                edge(
                    "lose",
                    "shared",
                    "failure",
                    "空手对峙",
                    json!({"requires":{"evidence_secured":false}}),
                ),
            ],
        )
        .expect("save graph");

    let success = follow_path(&repository, game_id, "保留证据");
    assert_eq!(success["current_node"]["node_type"], "success");
    assert_eq!(success["state"]["evidence_secured"], true);
    let failure = follow_path(&repository, game_id, "丢弃证据");
    assert_eq!(failure["current_node"]["node_type"], "failure");
    assert_eq!(failure["state"]["evidence_secured"], false);
    fs::remove_dir_all(root).expect("remove test data");
}

fn follow_path(repository: &Repository, game_id: &str, opening_option: &str) -> serde_json::Value {
    let session = repository
        .create_game_session(game_id)
        .expect("start session");
    let session_id = session["id"].as_str().expect("session id");
    let shared = repository
        .choose_game_edge(game_id, session_id, &edge_id(&session, opening_option))
        .expect("take opening choice");
    assert_eq!(shared["choices"].as_array().expect("choices").len(), 1);
    repository
        .choose_game_edge(
            game_id,
            session_id,
            shared["choices"][0]["id"].as_str().expect("later edge"),
        )
        .expect("take conditional choice")
}

fn edge_id(session: &serde_json::Value, option_text: &str) -> String {
    session["choices"]
        .as_array()
        .expect("choices")
        .iter()
        .find(|choice| choice["option_text"] == option_text)
        .and_then(|choice| choice["id"].as_str())
        .expect("opening edge")
        .to_owned()
}

fn node(id: &str, node_type: &str) -> serde_json::Value {
    json!({"id":id,"node_type":node_type,"title":id,"original_text":id,"prompt":id,"duration_seconds":5})
}

fn edge(
    id: &str,
    source: &str,
    target: &str,
    text: &str,
    conditions: serde_json::Value,
) -> serde_json::Value {
    json!({"id":id,"source_node_id":source,"target_node_id":target,"option_text":text,"sort_order":1,"conditions":conditions})
}
