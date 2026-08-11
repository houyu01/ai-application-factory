//! Regression coverage ensuring failed interactive-game generation never writes fallback content.

use std::fs;

use serde_json::{json, Map};

use crate::{
    db::Database,
    media::MediaStore,
    planner,
    repository::Repository,
    value::{new_id, FAILED, GENERATING},
    worker::DurableWorker,
};

fn repository() -> (Repository, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let database = Database::open(root.join("ai_application_factory.db")).expect("test database");
    (Repository::new(database), root)
}

fn game(repository: &Repository) -> serde_json::Value {
    repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("失败不兜底")),
            (
                "script".to_owned(),
                json!("玩家在钟楼收到失踪搭档的录音，并在两条线索之间做出选择。"),
            ),
            ("success_ending_count".to_owned(), json!(1)),
            ("failure_ending_count".to_owned(), json!(1)),
        ]))
        .expect("create game")
}

fn worker(repository: Repository) -> DurableWorker {
    DurableWorker::new(
        repository.clone(),
        MediaStore::new(repository).expect("media store"),
    )
    .expect("worker")
}

#[test]
fn graph_plan_rejects_repeated_story_text_and_keeps_saved_prompts_unique() {
    let game = json!({
        "success_ending_count": 1,
        "failure_ending_count": 1,
        "branch_min": 2,
        "branch_max": 2,
        "node_duration_min": 5,
        "node_duration_max": 10,
    });
    let mut response = json!({
        "assets": [],
        "nodes": [
            {"id":"start","node_type":"start","title":"入口","original_text":"调查员推开钟楼木门。","prompt":"镜头跟随调查员进入钟楼。","duration_seconds":5},
            {"id":"success","node_type":"success","title":"成功","original_text":"调查员在顶层公开录音。","prompt":"镜头跟随调查员进入钟楼。","duration_seconds":5},
            {"id":"failure","node_type":"failure","title":"失败","original_text":"调查员触发警铃被困。","prompt":"镜头跟随调查员进入钟楼。","duration_seconds":5},
        ],
        "edges": [
            {"id":"success","source_node_id":"start","target_node_id":"success","option_text":"带着录音登上顶层"},
            {"id":"failure","source_node_id":"start","target_node_id":"failure","option_text":"贸然触碰警铃"},
        ],
    });

    let plan = planner::model_game_plan(&response.to_string(), &game).expect("unique graph");
    let prompts = plan["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|node| node["prompt"].as_str().expect("prompt"))
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(prompts.len(), 3);

    response["nodes"][1]["original_text"] = response["nodes"][0]["original_text"].clone();
    assert!(planner::model_game_plan(&response.to_string(), &game).is_none());
}

#[test]
fn failed_screenplay_expansion_is_visible_without_a_fallback() {
    let (repository, root) = repository();
    let game = game(&repository);
    let game_id = game["id"].as_str().expect("game id");

    assert!(worker(repository.clone())
        .process_once()
        .expect("process task"));

    let failed = repository.get_game(game_id).expect("load game");
    let task = &failed["tasks"][0];
    assert_eq!(failed["status"], FAILED);
    assert!(failed["nodes"].as_array().is_some_and(Vec::is_empty));
    assert_eq!(task["status"], FAILED);
    assert!(task["error_message"]
        .as_str()
        .is_some_and(|error| error.contains("未返回互动游戏剧本")));
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn failed_graph_generation_does_not_write_a_fallback_graph() {
    let (repository, root) = repository();
    let game = game(&repository);
    let game_id = game["id"].as_str().expect("game id");
    repository
        .complete_game_screenplay_expansion(
            game["task"]["id"].as_str().expect("expansion task"),
            game_id,
            "【剧情段 S01｜开始】\n剧情正文：钟楼警报响起。\n【玩家抉择】\n【结局 E01｜成功】\n【结局 E02｜失败】",
            20,
            true,
        )
        .expect("complete expansion");

    assert!(worker(repository.clone())
        .process_once()
        .expect("process graph"));

    let failed = repository.get_game(game_id).expect("load game");
    let task = failed["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .find(|task| task["type"] == "game_graph_decomposition")
        .expect("graph task");
    assert_eq!(failed["status"], FAILED);
    assert!(failed["nodes"].as_array().is_some_and(Vec::is_empty));
    assert_eq!(task["status"], FAILED);
    assert!(task["error_message"]
        .as_str()
        .is_some_and(|error| error.contains("未返回游戏图谱")));
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn failed_node_prompt_keeps_content_and_retries_only_that_node() {
    let (repository, root) = repository();
    let game = game(&repository);
    let game_id = game["id"].as_str().expect("game id");
    repository
        .complete_game_screenplay_expansion(
            game["task"]["id"].as_str().expect("expansion task"),
            game_id,
            "已保存剧本",
            20,
            false,
        )
        .expect("complete expansion");
    repository
        .save_game_graph(
            game_id,
            &[],
            &[
                json!({"id":"one","node_type":"start","title":"节点一","original_text":"第一段独立剧情。","prompt":"提示词一","duration_seconds":10}),
                json!({"id":"two","node_type":"normal","title":"节点二","original_text":"第二段独立剧情。","prompt":"提示词二","duration_seconds":10}),
            ],
            &[],
        )
        .expect("save graph");
    let saved = repository.get_game(game_id).expect("load graph");
    let node_id = saved["nodes"][0]["id"].as_str().expect("node id");
    let other_id = saved["nodes"][1]["id"].as_str().expect("other node id");
    let before = repository
        .get_game_node(game_id, node_id)
        .expect("node before");
    let other_before = repository
        .get_game_node(game_id, other_id)
        .expect("other before");
    let task = repository
        .enqueue_game_node_prompt(game_id, node_id)
        .expect("queue node prompt");

    assert!(worker(repository.clone())
        .process_once()
        .expect("process node prompt"));

    let failed = repository
        .get_game_task(task["id"].as_str().expect("task id"))
        .expect("failed task");
    assert_eq!(failed["status"], FAILED);
    assert!(failed["error_message"]
        .as_str()
        .is_some_and(|error| error.contains("仅重试当前节点")));
    assert_eq!(
        repository
            .get_game_node(game_id, node_id)
            .expect("node after")["prompt"],
        before["prompt"]
    );
    assert_eq!(
        repository
            .get_game_node(game_id, other_id)
            .expect("other after")["prompt"],
        other_before["prompt"]
    );
    let retry = repository
        .enqueue_game_node_prompt(game_id, node_id)
        .expect("retry node prompt");
    assert_ne!(retry["id"], task["id"]);
    assert_eq!(retry["status"], GENERATING);
    assert_eq!(retry["resource_id"], node_id);
    fs::remove_dir_all(root).expect("remove test data");
}
