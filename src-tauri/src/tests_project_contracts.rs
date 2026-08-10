//! Regression coverage for bounded editor and project-library persistence projections.

use std::fs;

use serde_json::{json, Map};

use crate::{
    db::Database,
    planner,
    repository::Repository,
    value::{new_id, CANCELLED, FAILED, GENERATING, SUCCEEDED},
};

fn repository() -> (Repository, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let database = Database::open(root.join("ai_application_factory.db")).expect("test database");
    (Repository::new(database), root)
}

#[test]
fn project_list_editor_projection_and_model_queues_keep_python_contracts() {
    let (repository, root) = repository();
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("详情投影短剧")),
            (
                "script".to_owned(),
                json!("林岩在旧宅找到钥匙，苏晚赶来阻止他继续调查。"),
            ),
        ]))
        .expect("create project");
    let id = project["id"].as_str().expect("project id");
    assert_eq!(project["task"]["stage"], "");
    let listed = repository.list_dramas().expect("project list");
    assert_eq!(listed[0]["script"], "");
    assert_eq!(listed[0]["queue_position"], 1);
    assert_eq!(listed[0]["queue_state"], "queued");
    assert!(listed[0]["tasks"].is_null());

    let bootstrap = project["task_id"].as_str().expect("bootstrap id");
    let plan = planner::fallback_drama_plan(
        "林岩在旧宅找到钥匙，苏晚赶来阻止他继续调查。",
        "真人风格",
        "都市",
        80,
    );
    repository
        .save_drama_decomposition(id, &plan)
        .expect("save plan");
    repository
        .finish_drama_task(bootstrap, SUCCEEDED, None, None)
        .expect("finish bootstrap");
    let detail = repository.get_drama(id).expect("detail");
    let shot_id = detail["shots"][0]["id"].as_str().expect("shot id");
    let version = repository
        .create_shot_version(id, shot_id, "task-version", "version prompt")
        .expect("version");
    repository
        .finish_shot_version(
            id,
            shot_id,
            version["id"].as_str().expect("version id"),
            SUCCEEDED,
            Some("/api/media/video.mp4"),
            None,
        )
        .expect("finish version");
    assert_eq!(
        repository.get_shot(id, shot_id).expect("shot")["historical_videos"],
        json!([]),
    );
    assert_eq!(
        repository
            .shot_versions(id, shot_id)
            .expect("versions")
            .len(),
        1,
    );
    repository
        .save_generated_shot_prompt(
            id,
            shot_id,
            "更新后的提示词",
            &[],
            &json!({}),
            &[],
            None,
            "v1",
        )
        .expect("save prompt without clearing completed video state");
    assert_eq!(
        repository.get_shot(id, shot_id).expect("shot")["status"],
        SUCCEEDED
    );
    let editor = repository
        .get_editor_drama(id, Some(shot_id))
        .expect("editor detail");
    assert_eq!(editor["script"], "");
    assert!(!editor["shots"][0]["prompt"]
        .as_str()
        .expect("selected prompt")
        .is_empty());
    assert_eq!(editor["shots"][0]["versions"][0]["task_id"], "task-version");
    if editor["shots"].as_array().expect("shots").len() > 1 {
        assert_eq!(editor["shots"][1]["prompt"], "");
    }

    let asset_id = detail["assets"][0]["id"].as_str().expect("asset id");
    repository
        .create_active_drama_task(id, "asset_image", Some(asset_id), json!({}))
        .expect("image task");
    repository
        .create_parallel_drama_task(id, "shot_video", Some(shot_id), json!({}))
        .expect("video task");
    let video = repository
        .claim_drama_task_types(&["shot_video"])
        .expect("claim video")
        .expect("video task present");
    assert_eq!(video["type"], "shot_video");
    let image = repository
        .claim_drama_task_types(&["asset_image"])
        .expect("claim image")
        .expect("image task present");
    assert_eq!(image["type"], "asset_image");
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn task_progress_retains_its_claim_until_the_worker_finishes() {
    let (repository, root) = repository();
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("租约保护短剧")),
            (
                "script".to_owned(),
                json!("主角在雨夜收到一封旧信，决定追查信件的来历。"),
            ),
        ]))
        .expect("create project");
    let task_id = project["task_id"].as_str().expect("bootstrap task id");
    let claimed = repository
        .claim_drama_task_types(&["script_decomposition"])
        .expect("claim task")
        .expect("task available");
    let token = claimed["poll_lease_token"].as_str().expect("lease token");

    repository
        .update_drama_task_progress(task_id, 10, "正在准备剧本")
        .expect("write progress");
    let persisted = repository
        .get_drama_task(task_id)
        .expect("task after progress");
    assert_eq!(persisted["poll_lease_token"], token);
    assert!(repository
        .claim_drama_task_types(&["script_decomposition"])
        .expect("read queue")
        .is_none());
    assert!(!repository
        .renew_drama_task_lease(task_id, "another-worker")
        .expect("reject other worker"));
    assert!(repository
        .renew_drama_task_lease(task_id, token)
        .expect("renew active worker"));
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn failed_story_bible_keeps_preview_and_can_be_restarted() {
    let (repository, root) = repository();
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("可重试故事圣经")),
            (
                "script".to_owned(),
                json!("主角为寻找失踪的师父踏上旅程，在每一集面对新的选择与代价。"),
            ),
        ]))
        .expect("create project");
    let project_id = project["id"].as_str().expect("project id");
    let task_id = project["task_id"].as_str().expect("task id");
    repository
        .update_drama_task_snapshot(task_id, json!({"story_bible_preview":"第001集：雨夜来信"}))
        .expect("save story bible preview");
    repository
        .finish_drama_task(task_id, FAILED, None, Some("故事圣经生成超时"))
        .expect("fail task");

    let failed = repository.get_drama(project_id).expect("failed detail");
    assert_eq!(
        failed["tasks"][0]["input_snapshot"]["story_bible_preview"],
        "第001集：雨夜来信"
    );
    let retried = repository
        .retry_drama_task(project_id, "script_decomposition")
        .expect("retry task");
    assert_eq!(retried["status"], GENERATING);
    assert_eq!(retried["stage"], "等待重试");
    assert_eq!(
        repository.get_drama(project_id).expect("retried detail")["status"],
        GENERATING
    );
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn cancelled_bootstrap_can_start_a_fresh_task_without_reusing_its_preview() {
    let (repository, root) = repository();
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("重新开始短剧")),
            (
                "script".to_owned(),
                json!("主角在暴雨夜收到一段录音，循着声音找回失散多年的家人。"),
            ),
        ]))
        .expect("create project");
    let project_id = project["id"].as_str().expect("project id");
    let original_task_id = project["task_id"].as_str().expect("task id");
    repository
        .update_drama_task_snapshot(
            original_task_id,
            json!({"story_bible_preview":"已取消的旧输出"}),
        )
        .expect("preview");
    repository
        .cancel_drama_task(original_task_id, "任务已取消")
        .expect("cancel task");
    let restarted = repository
        .restart_drama_task(project_id, "script_decomposition")
        .expect("restart task");
    assert_ne!(restarted["id"], original_task_id);
    assert_eq!(restarted["status"], GENERATING);
    assert_eq!(restarted["stage"], "等待重新生成");
    assert_eq!(restarted["input_snapshot"]["drama_id"], project_id);
    assert!(restarted["input_snapshot"]["story_bible_preview"].is_null());
    let detail = repository.get_drama(project_id).expect("restarted detail");
    assert_eq!(detail["status"], GENERATING);
    assert_eq!(detail["tasks"][0]["status"], CANCELLED);
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn completed_project_can_regenerate_its_full_editor_graph_from_source() {
    let (repository, root) = repository();
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("完整重新生成短剧")),
            (
                "script".to_owned(),
                json!("主角在旧宅找到一把钥匙，决定和朋友一同追查钥匙背后的秘密。"),
            ),
        ]))
        .expect("create project");
    let project_id = project["id"].as_str().expect("project id");
    let bootstrap_id = project["task_id"].as_str().expect("bootstrap task id");
    let plan = planner::fallback_drama_plan(
        "主角在旧宅找到一把钥匙，决定和朋友一同追查钥匙背后的秘密。",
        "真人风格",
        "都市",
        80,
    );
    repository
        .save_drama_decomposition(project_id, &plan)
        .expect("save original decomposition");
    repository
        .finish_drama_task(bootstrap_id, SUCCEEDED, None, None)
        .expect("finish original bootstrap");
    let detail = repository.get_drama(project_id).expect("original detail");
    let image_task = repository
        .create_active_drama_task(
            project_id,
            "asset_image",
            detail["assets"][0]["id"].as_str(),
            json!({}),
        )
        .expect("active image task");

    let regenerated = repository
        .regenerate_drama(
            project_id,
            Some("主角收到一张旧照片后，踏上寻找照片中失踪亲人的旅程。"),
        )
        .expect("regenerate project");
    let refreshed = repository
        .get_drama(project_id)
        .expect("regenerated detail");
    let screenplay = repository
        .get_expanded_screenplay(project_id)
        .expect("regenerated screenplay");
    assert_eq!(regenerated["type"], "script_decomposition");
    assert_eq!(regenerated["status"], GENERATING);
    assert_eq!(regenerated["input_snapshot"]["drama_id"], project_id);
    assert_eq!(refreshed["status"], GENERATING);
    assert_eq!(
        refreshed["script"],
        "主角收到一张旧照片后，踏上寻找照片中失踪亲人的旅程。"
    );
    assert_eq!(screenplay["expanded_script"], "");
    assert!(refreshed["assets"].as_array().expect("assets").is_empty());
    assert!(refreshed["shots"].as_array().expect("shots").is_empty());
    assert_eq!(
        repository
            .get_drama_task(image_task["id"].as_str().expect("image task id"))
            .expect("cancelled image task")["status"],
        CANCELLED
    );
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn screenplay_continuation_polling_keeps_its_live_preview() {
    let (repository, root) = repository();
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("续写实时输出")),
            (
                "script".to_owned(),
                json!("主角从旧信里发现线索，决定连夜前往山中的废弃车站。"),
            ),
        ]))
        .expect("create project");
    let project_id = project["id"].as_str().expect("project id");
    repository
        .finish_drama_task(
            project["task_id"].as_str().expect("bootstrap task id"),
            SUCCEEDED,
            None,
            None,
        )
        .expect("finish bootstrap");
    let continuation = repository
        .create_active_drama_task(
            project_id,
            "script_expansion",
            None,
            json!({"expanded_script_preview":"【第001集：旧信】\\n车站的灯亮了起来。"}),
        )
        .expect("create continuation");
    repository
        .update_drama_task_progress(
            continuation["id"].as_str().expect("continuation id"),
            12,
            "正在继续扩写剧本（已接收 12 字）",
        )
        .expect("save preview progress");

    let tasks = repository
        .poll_drama_tasks(project_id, Some(GENERATING), None)
        .expect("poll tasks");
    let continuation = tasks["tasks"]
        .as_array()
        .expect("task array")
        .iter()
        .find(|task| task["type"] == "script_expansion")
        .expect("continuation task");
    assert_eq!(
        continuation["input_snapshot"]["expanded_script_preview"],
        "【第001集：旧信】\\n车站的灯亮了起来。"
    );
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn validation_rejects_out_of_range_project_numbers() {
    let (repository, root) = repository();
    let result = repository.create_drama(Map::from_iter([
        ("name".to_owned(), json!("限制校验")),
        (
            "script".to_owned(),
            json!("这是一个长度足够的剧本开头，用于校验创建参数。"),
        ),
        ("episode_count".to_owned(), json!(101)),
    ]));
    assert!(result.is_err());
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn decomposition_keeps_each_asset_prompt_and_character_voice() {
    let (repository, root) = repository();
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("素材拆解")),
            (
                "script".to_owned(),
                json!("苏晚在旧车站发现一封信，并决定追查写信的人。"),
            ),
        ]))
        .expect("create project");
    let project_id = project["id"].as_str().expect("project id");
    let plan = json!({"episodes":[],"assets":[
        {"id":"character","type":"character","name":"苏晚","prompt":"角色独立外观描述","voice_id":"strong_female_lead"},
        {"id":"scene","type":"scene","name":"旧车站","prompt":"场景独立空间描述"},
        {"id":"prop","type":"prop","name":"线索信","prompt":"道具独立材质描述"}
    ]});

    repository
        .save_drama_decomposition(project_id, &plan)
        .expect("save decomposition");
    let assets = repository.get_drama(project_id).expect("detail")["assets"]
        .as_array()
        .cloned()
        .expect("assets");
    let asset = |kind| {
        assets
            .iter()
            .find(|asset| asset["type"] == kind)
            .expect("asset")
    };
    assert_eq!(asset("character")["voice_id"], "strong_female_lead");
    assert_eq!(asset("character")["prompt"], "角色独立外观描述");
    assert_eq!(asset("scene")["prompt"], "场景独立空间描述");
    assert_eq!(asset("prop")["prompt"], "道具独立材质描述");
    fs::remove_dir_all(root).expect("remove test data");
}
