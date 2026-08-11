//! Regression coverage for durable interactive-game node-video provider polling.

use std::fs;

use rusqlite::Connection;
use serde_json::{json, Map};

use crate::{
    db::Database,
    repository::Repository,
    value::{new_id, CANCELLED, GENERATING, SUCCEEDED},
};

#[test]
fn pending_game_video_provider_job_remains_generating_until_its_poll_is_due() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("视频任务")),
            (
                "script".to_owned(),
                json!("玩家在钟楼的岔路前选择一扇门，并承担不同的后果。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    repository
        .save_game_graph(
            game_id,
            &[],
            &[json!({"id":"start","node_type":"start","title":"钟楼入口","original_text":"进入钟楼","prompt":"人物进入钟楼","duration_seconds":10})],
            &[],
        )
        .expect("save game graph");
    let node_id = repository.get_game(game_id).expect("load game")["nodes"][0]["id"]
        .as_str()
        .expect("node id")
        .to_owned();
    let task = repository
        .enqueue_game_node_video(game_id, &node_id)
        .expect("queue video");
    assert!(task["input_snapshot"]["prompt"]
        .as_str()
        .is_some_and(|prompt| prompt.contains("原始剧情依据（必须画面化）：进入钟楼")));
    repository
        .schedule_game_provider_poll(
            task["id"].as_str().expect("task id"),
            "remote-video-123",
            12,
            "正在等待节点视频生成结果",
        )
        .expect("schedule provider poll");
    let task = repository
        .get_game_task(task["id"].as_str().expect("task id"))
        .expect("load task");
    assert_eq!(task["status"], GENERATING);
    assert_eq!(task["provider_task_id"], "remote-video-123");
    assert!(repository
        .claim_game_task_types(&["node_video_generation"])
        .expect("claim game video")
        .is_none());
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn cancelling_a_game_video_task_keeps_the_last_playable_node_video() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("取消节点视频")),
            (
                "script".to_owned(),
                json!("玩家在密室中寻找出口，并根据线索选择不同路线。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    repository
        .save_game_graph(
            game_id,
            &[],
            &[json!({"id":"start","node_type":"start","title":"密室入口","original_text":"进入密室","prompt":"人物进入密室","video_url":"/media/previous.mp4","duration_seconds":17})],
            &[],
        )
        .expect("save game graph");
    let node_id = repository.get_game(game_id).expect("game")["nodes"][0]["id"]
        .as_str()
        .expect("node id")
        .to_owned();
    assert_eq!(
        repository.get_game_node(game_id, &node_id).expect("node")["duration_seconds"],
        15
    );
    repository
        .enqueue_game_node_video(game_id, &node_id)
        .expect("queue video");
    let task = repository
        .cancel_game_node_video_task(game_id, &node_id)
        .expect("cancel video");
    let node = repository
        .get_game_node(game_id, &node_id)
        .expect("node after cancel");
    assert_eq!(task["status"], CANCELLED);
    assert_eq!(node["status"], CANCELLED);
    assert_eq!(node["video_url"], "/media/previous.mp4");
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn cancelling_all_game_video_tasks_keeps_a_cancelled_history_for_each_node() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("取消所有节点视频")),
            (
                "script".to_owned(),
                json!("玩家在迷宫的两条岔路中探索，分别触发不同的机关与后果。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    repository
        .save_game_graph(
            game_id,
            &[],
            &[
                json!({"id":"first","node_type":"start","title":"入口","original_text":"进入迷宫","prompt":"玩家走入迷宫入口","duration_seconds":10}),
                json!({"id":"second","node_type":"normal","title":"岔路","original_text":"选择岔路","prompt":"玩家在石门前停下并选择方向","duration_seconds":10}),
            ],
            &[],
        )
        .expect("save graph");
    let nodes = repository.get_game(game_id).expect("game")["nodes"]
        .as_array()
        .expect("nodes")
        .to_vec();
    for node in &nodes {
        repository
            .enqueue_game_node_video(game_id, node["id"].as_str().expect("node id"))
            .expect("queue video");
    }
    let cancelled = repository
        .cancel_all_game_node_video_tasks(game_id)
        .expect("cancel all videos");
    assert_eq!(cancelled.len(), 2);
    for node in nodes {
        let node = repository
            .get_game_node(game_id, node["id"].as_str().expect("node id"))
            .expect("cancelled node");
        assert_eq!(node["status"], CANCELLED);
        assert_eq!(node["video_history"][0]["status"], CANCELLED);
    }
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn game_video_history_retains_refinement_inputs_and_restores_previous_version_after_delete() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("节点版本历史")),
            (
                "script".to_owned(),
                json!("玩家在灯塔中探索，逐步发现两条截然不同的逃生路线。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    repository
        .save_game_graph(
            game_id,
            &[],
            &[json!({"id":"start","node_type":"start","title":"灯塔入口","original_text":"进入灯塔","prompt":"人物走入雨夜灯塔，镜头缓慢推进","duration_seconds":10})],
            &[],
        )
        .expect("save graph");
    let node_id = repository.get_game(game_id).expect("game")["nodes"][0]["id"]
        .as_str()
        .expect("node id")
        .to_owned();
    let first = repository
        .enqueue_game_node_video(game_id, &node_id)
        .expect("first task");
    let first_id = first["id"].as_str().expect("first task id").to_owned();
    repository
        .finish_game_node_video(
            game_id,
            &node_id,
            &first_id,
            Some("/media/first.mp4"),
            SUCCEEDED,
            None,
        )
        .expect("finish first");
    repository
        .finish_game_task(&first_id, SUCCEEDED, None, None)
        .expect("finish first task");
    let second = repository
        .enqueue_game_node_video_refinement(
            game_id,
            &node_id,
            &first_id,
            "让灯光更温暖，镜头推进更慢",
        )
        .expect("refinement task");
    assert_eq!(
        second["input_snapshot"]["refinement"]["source_video_id"],
        first_id
    );
    repository
        .select_game_node_video_for_use(game_id, &node_id, &first_id)
        .expect("select first version for use");
    repository
        .finish_game_node_video(
            game_id,
            &node_id,
            second["id"].as_str().expect("second id"),
            Some("/media/second.mp4"),
            SUCCEEDED,
            None,
        )
        .expect("finish second");
    let node = repository
        .get_game_node(game_id, &node_id)
        .expect("node after selected second result");
    assert_eq!(node["selected_video_id"], first_id);
    assert_eq!(node["video_url"], "/media/first.mp4");
    let deleted = repository
        .delete_game_node_video(game_id, &node_id, second["id"].as_str().expect("second id"))
        .expect("delete second");
    assert_eq!(deleted["status"], "deleted");
    let node = repository
        .get_game_node(game_id, &node_id)
        .expect("node after delete");
    assert_eq!(node["selected_video_id"], first_id);
    assert_eq!(node["video_url"], "/media/first.mp4");
    assert_eq!(node["video_history"].as_array().expect("history").len(), 1);
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn reopening_repairs_legacy_game_video_durations_before_a_task_is_submitted() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let database_path = root.join("ai_application_factory.db");
    let repository = Repository::new(Database::open(database_path.clone()).expect("test database"));
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("修复旧视频时长")),
            (
                "script".to_owned(),
                json!("玩家在风暴中决定是否点燃灯塔，引导远处的船只靠岸。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id").to_owned();
    repository
        .save_game_graph(
            &game_id,
            &[],
            &[json!({"id":"start","node_type":"start","title":"灯塔","original_text":"点亮灯塔","prompt":"暴风雨中的灯塔","duration_seconds":10})],
            &[],
        )
        .expect("save game graph");
    let node_id = repository.get_game(&game_id).expect("game")["nodes"][0]["id"]
        .as_str()
        .expect("node id")
        .to_owned();
    drop(repository);

    let connection = Connection::open(&database_path).expect("open legacy database");
    connection
        .execute(
            "UPDATE interactive_games SET node_duration_min=3,node_duration_max=30 WHERE id=?1",
            [&game_id],
        )
        .expect("write legacy game duration");
    connection
        .execute(
            "UPDATE game_nodes SET duration_seconds=17 WHERE id=?1",
            [&node_id],
        )
        .expect("write legacy node duration");
    connection
        .execute(
            "DELETE FROM desktop_schema_migrations WHERE id='game_video_duration_range_v1'",
            [],
        )
        .expect("reset duration migration");
    drop(connection);

    let repaired = Repository::new(Database::open(database_path).expect("reopen database"));
    let game = repaired.get_game(&game_id).expect("repaired game");
    let node = repaired
        .get_game_node(&game_id, &node_id)
        .expect("repaired node");
    assert_eq!(game["node_duration_min"], 4);
    assert_eq!(game["node_duration_max"], 15);
    assert_eq!(node["duration_seconds"], 15);
    drop(repaired);
    fs::remove_dir_all(root).expect("remove test data");
}
