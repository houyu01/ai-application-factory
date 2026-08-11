//! Regression coverage for replacing a completed interactive-game run from revised source text.

use std::fs;

use serde_json::{json, Map};

use crate::{
    db::Database,
    repository::Repository,
    value::{new_id, CANCELLED, GENERATING},
};

#[test]
fn regenerating_a_game_clears_derived_state_and_queues_a_fresh_expansion() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("钟楼回声")),
            (
                "script".to_owned(),
                json!("玩家在钟楼收到失踪搭档的录音，并在两条线索之间做出选择。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    let initial_task_id = game["task"]["id"].as_str().expect("initial task id");
    repository
        .complete_game_screenplay_expansion(
            initial_task_id,
            game_id,
            "【剧情段 S01｜开始】\n剧情正文：钟楼警报响起。\n【结局 E01｜成功】",
            20,
            true,
        )
        .expect("queue graph decomposition");
    let graph_task_id = repository.get_game(game_id).expect("load graph task")["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .find(|task| task["type"] == "game_graph_decomposition")
        .and_then(|task| task["id"].as_str())
        .expect("graph task id")
        .to_owned();
    repository
        .save_game_graph(
            game_id,
            &[json!({"id":"recording","type":"prop","name":"录音机","prompt":"磨损的便携录音机"})],
            &[
                json!({"id":"start","node_type":"start","title":"钟楼入口","original_text":"拿起录音机。","prompt":"钟楼入口","duration_seconds":10}),
                json!({"id":"success","node_type":"success","title":"真相","original_text":"公开录音。","prompt":"钟楼顶层","duration_seconds":10}),
            ],
            &[json!({"id":"enter","source_node_id":"start","target_node_id":"success","option_text":"带着录音上楼","sort_order":1})],
        )
        .expect("save graph");
    let graph = repository.get_game(game_id).expect("load graph");
    let start_id = graph["nodes"][0]["id"].as_str().expect("start node id");
    let prompt_task = repository
        .enqueue_game_node_prompt(game_id, start_id)
        .expect("queue node prompt");
    let session = repository
        .create_game_session(game_id)
        .expect("create session");
    repository
        .choose_game_edge(
            game_id,
            session["id"].as_str().expect("session id"),
            session["choices"][0]["id"].as_str().expect("choice id"),
        )
        .expect("record choice");

    let regenerated = repository
        .regenerate_game_screenplay(
            game_id,
            Map::from_iter([(
                "script".to_owned(),
                json!("玩家收到一张旧照片后，决定在暴雨前赶往山中车站寻找失踪亲人。"),
            )]),
        )
        .expect("regenerate game");
    assert!(repository
        .save_generated_game_graph(
            &graph_task_id,
            game_id,
            &[],
            &[json!({"id":"stale","node_type":"start","title":"旧节点","original_text":"旧节点。","prompt":"旧节点","duration_seconds":10})],
            &[],
        )
        .is_err());
    let refreshed = repository.get_game(game_id).expect("load regenerated game");

    assert_eq!(regenerated["type"], "game_script_expansion");
    assert_eq!(regenerated["status"], GENERATING);
    assert_eq!(regenerated["stage"], "等待重新生成");
    assert_eq!(regenerated["input_snapshot"]["game_id"], game_id);
    assert_eq!(refreshed["status"], GENERATING);
    assert_eq!(refreshed["expanded_script"], "");
    assert_eq!(
        refreshed["script"],
        "玩家收到一张旧照片后，决定在暴雨前赶往山中车站寻找失踪亲人。"
    );
    assert!(refreshed["assets"].as_array().expect("assets").is_empty());
    assert!(refreshed["nodes"].as_array().expect("nodes").is_empty());
    assert!(refreshed["edges"].as_array().expect("edges").is_empty());
    assert_eq!(
        repository
            .get_game_task(&graph_task_id)
            .expect("graph task")["status"],
        CANCELLED
    );
    assert_eq!(
        repository
            .get_game_task(prompt_task["id"].as_str().expect("prompt task id"))
            .expect("prompt task")["status"],
        CANCELLED
    );
    let counts: (i64, i64, i64, i64, i64) = repository.db.with_connection(|connection| {
        connection.query_row(
            "SELECT (SELECT COUNT(*) FROM game_assets WHERE game_id=?1),(SELECT COUNT(*) FROM game_nodes WHERE game_id=?1),(SELECT COUNT(*) FROM game_edges WHERE game_id=?1),(SELECT COUNT(*) FROM game_sessions WHERE game_id=?1),(SELECT COUNT(*) FROM game_choice_events WHERE game_id=?1)",
            [game_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        ).map_err(Into::into)
    }).expect("count derived rows");
    assert_eq!(counts, (0, 0, 0, 0, 0));
    fs::remove_dir_all(root).expect("remove test data");
}
