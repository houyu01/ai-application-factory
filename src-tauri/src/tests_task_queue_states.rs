//! Regression coverage for visible queue state and restart recovery of durable task leases.

use std::fs;

use serde_json::{json, Map};

use crate::{
    db::Database,
    repository::Repository,
    value::{new_id, now, FAILED, GENERATING},
};

#[test]
fn drama_tasks_wait_before_claim_and_recover_expired_legacy_leases() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let path = root.join("ai_application_factory.db");
    let database = Database::open(path.clone()).expect("test database");
    let repository = Repository::new(database.clone());
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("队列状态短剧")),
            (
                "script".to_owned(),
                json!("主角在雨夜收到一封旧信，决定查清信件的来历。"),
            ),
        ]))
        .expect("create project");
    let task = repository
        .create_active_drama_task(
            project["id"].as_str().expect("project id"),
            "shot_prompt",
            Some("queued-shot"),
            json!({}),
        )
        .expect("queue prompt task");
    let task_id = task["id"].as_str().expect("task id");

    assert_eq!(
        repository.get_drama_task(task_id).expect("queued task")["stage"],
        "等待队列"
    );
    let claimed = repository
        .claim_drama_task_types(&["shot_prompt"])
        .expect("claim queued task")
        .expect("task available");
    assert_eq!(claimed["stage"], "正在执行");

    database
        .with_connection(|connection| {
            connection.execute(
                "UPDATE generation_tasks SET stage='',poll_lease_token='abandoned',poll_lease_until='2000-01-01T00:00:00Z' WHERE id=?1",
                [task_id],
            )?;
            Ok(())
        })
        .expect("seed expired legacy lease");
    Database::open(path).expect("restart recovers task queue");
    assert_eq!(
        repository.get_drama_task(task_id).expect("recovered task")["stage"],
        "等待队列"
    );
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn restart_resumes_remote_video_polls_and_exposes_interrupted_image_retry() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let path = root.join("ai_application_factory.db");
    let database = Database::open(path.clone()).expect("test database");
    let repository = Repository::new(database.clone());
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("重启恢复短剧")),
            ("script".to_owned(), json!("主角在暴雨中寻找失踪的姐姐。")),
        ]))
        .expect("create project");
    let project_id = project["id"].as_str().expect("project id");
    let asset_id = "interrupted-image";
    database
        .with_connection(|connection| {
            let timestamp = now();
            connection.execute(
                "INSERT INTO drama_assets (id,drama_id,type,name,prompt,variants_json,status,created_at,updated_at) VALUES (?1,?2,'scene','雨夜街道','雨夜街道','[]',?3,?4,?4)",
                rusqlite::params![asset_id, project_id, GENERATING, timestamp],
            )?;
            Ok(())
        })
        .expect("seed generating asset");
    let image = repository
        .create_active_drama_task(project_id, "asset_image", Some(asset_id), json!({}))
        .expect("queue image");
    let image_id = image["id"].as_str().expect("image task id");
    repository
        .claim_drama_task_types(&["asset_image"])
        .expect("claim image task");
    let video = repository
        .create_active_drama_task(project_id, "shot_video", Some("shot-1"), json!({}))
        .expect("queue video");
    let video_id = video["id"].as_str().expect("video task id");
    repository
        .schedule_drama_provider_poll(video_id, "remote-video-123", 42, "等待视频结果")
        .expect("persist remote video id");

    Database::open(path).expect("restart recovers task state");

    let recovered_image = repository
        .get_drama_task(image_id)
        .expect("recovered image");
    assert_eq!(recovered_image["status"], FAILED);
    assert!(recovered_image["error_message"]
        .as_str()
        .expect("image error")
        .contains("无法恢复"));
    assert_eq!(
        repository
            .get_asset(project_id, asset_id)
            .expect("recovered asset")["status"],
        FAILED
    );
    let recovered_video = repository
        .get_drama_task(video_id)
        .expect("recovered video");
    assert_eq!(recovered_video["status"], GENERATING);
    assert_eq!(recovered_video["provider_task_id"], "remote-video-123");
    assert_eq!(recovered_video["stage"], "正在恢复视频任务轮询");
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn game_expansion_with_an_expired_lease_becomes_retryable_failure() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let path = root.join("ai_application_factory.db");
    let database = Database::open(path).expect("test database");
    let repository = Repository::new(database.clone());
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("失联工作线程")),
            (
                "script".to_owned(),
                json!("玩家在钟楼发现失踪同伴的录音，需要在警报响起前找出出口。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    let task_id = game["task"]["id"].as_str().expect("task id");
    database
        .with_connection(|connection| {
            connection.execute(
                "UPDATE game_tasks SET poll_lease_token='lost-worker',poll_lease_until='2000-01-01T00:00:00Z' WHERE id=?1",
                [task_id],
            )?;
            Ok(())
        })
        .expect("seed expired game lease");

    let recovered = repository.get_game(game_id).expect("load recovered game");
    let task = recovered["tasks"]
        .as_array()
        .expect("game tasks")
        .iter()
        .find(|task| task["id"] == task_id)
        .expect("recovered task");
    assert_eq!(recovered["status"], FAILED);
    assert_eq!(task["status"], FAILED);
    assert!(task["error_message"]
        .as_str()
        .expect("failure reason")
        .contains("停止续租"));
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn restart_marks_in_progress_game_generation_failed() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let path = root.join("ai_application_factory.db");
    let database = Database::open(path.clone()).expect("test database");
    let repository = Repository::new(database);
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("重启中的游戏")),
            (
                "script".to_owned(),
                json!("玩家在废弃钟楼收到陌生录音，需要选择线索并找到失踪搭档。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    let task_id = game["task"]["id"].as_str().expect("task id");
    repository
        .update_game_task_snapshot(
            task_id,
            json!({"game_id":game_id,"expanded_script_preview":"已保存的扩写片段"}),
        )
        .expect("save checkpoint");

    Database::open(path).expect("restart recovers game task");
    let recovered = repository.get_game(game_id).expect("load recovered game");
    let task = recovered["tasks"]
        .as_array()
        .expect("game tasks")
        .iter()
        .find(|task| task["id"] == task_id)
        .expect("recovered task");
    assert_eq!(recovered["status"], FAILED);
    assert_eq!(task["status"], FAILED);
    assert_eq!(
        task["input_snapshot"]["expanded_script_preview"],
        "已保存的扩写片段"
    );
    assert!(task["error_message"]
        .as_str()
        .expect("failure reason")
        .contains("无法恢复"));
    fs::remove_dir_all(root).expect("remove test data");
}
