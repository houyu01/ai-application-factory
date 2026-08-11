//! Regression coverage for game-node boundary frames captured from related videos.

use std::fs;

use serde_json::{json, Map};

use crate::{
    db::Database,
    repository::Repository,
    value::{new_id, SUCCEEDED},
};

fn complete_video(repository: &Repository, game_id: &str, node_id: &str, url: &str) -> String {
    let task = repository
        .enqueue_game_node_video(game_id, node_id)
        .expect("video task");
    let video_id = task["id"].as_str().expect("video id").to_owned();
    repository
        .finish_game_node_video(game_id, node_id, &video_id, Some(url), SUCCEEDED, None)
        .expect("finish video");
    repository
        .finish_game_task(&video_id, SUCCEEDED, None, None)
        .expect("finish task");
    video_id
}

#[test]
fn related_video_frames_are_persisted_and_frozen_into_the_game_video_task() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("上游帧选择")),
            (
                "script".to_owned(),
                json!("玩家沿着废弃钟楼的线索探索，并在每一个岔路口做出影响结局的分支选择。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    repository
        .save_game_graph(
            game_id,
            &[],
            &[
                json!({"id":"source","node_type":"start","title":"入口","original_text":"走入钟楼","prompt":"走入钟楼","duration_seconds":5}),
                json!({"id":"target","node_type":"normal","title":"调查","original_text":"调查钟楼","prompt":"调查钟楼","duration_seconds":5}),
                json!({"id":"downstream","node_type":"success","title":"出口","original_text":"走出钟楼","prompt":"走出钟楼","duration_seconds":5}),
            ],
            &[
                json!({"id":"edge","source_node_id":"source","target_node_id":"target","option_text":"继续调查","sort_order":1}),
                json!({"id":"end","source_node_id":"target","target_node_id":"downstream","option_text":"前往出口","sort_order":1}),
            ],
        )
        .expect("save graph");
    let graph = repository.get_game(game_id).expect("game graph");
    let source_id = graph["nodes"][0]["id"].as_str().expect("source id");
    let target_id = graph["nodes"][1]["id"].as_str().expect("target id");
    let downstream_id = graph["nodes"][2]["id"].as_str().expect("downstream id");
    let source_video_id = complete_video(
        &repository,
        game_id,
        source_id,
        "https://example.com/source.mp4",
    );
    let downstream_video_id = complete_video(
        &repository,
        game_id,
        downstream_id,
        "https://example.com/downstream.mp4",
    );

    let node = repository
        .update_game_node(
            game_id,
            target_id,
            Map::from_iter([(
                "first_last_frames".to_owned(),
                json!({"first":{"url":"data:image/jpeg;base64,aGVsbG8=","source":"related_video","node_id":source_id,"video_id":source_video_id,"position":"last"},"last":{"url":"data:image/jpeg;base64,aGVsbG8tMg==","source":"related_video","node_id":downstream_id,"video_id":downstream_video_id,"position":"first"}}),
            )]),
        )
        .expect("save related video frames");
    assert_eq!(node["first_last_frames"]["first"]["node_id"], source_id);
    assert_eq!(node["first_last_frames"]["first"]["position"], "last");
    assert_eq!(node["first_last_frames"]["last"]["node_id"], downstream_id);
    assert_eq!(node["first_last_frames"]["last"]["position"], "first");

    let task = repository
        .enqueue_game_node_video(game_id, target_id)
        .expect("target task");
    assert_eq!(task["input_snapshot"]["frame_images"][0]["side"], "first");
    assert_eq!(
        task["input_snapshot"]["frame_images"][0]["url"],
        "data:image/jpeg;base64,aGVsbG8="
    );
    assert_eq!(task["input_snapshot"]["frame_images"][1]["side"], "last");
    fs::remove_dir_all(root).expect("remove test data");
}
