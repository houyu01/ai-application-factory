//! Local regression coverage for persisted desktop business flows.

use std::fs;

use serde_json::{json, Map, Value};

use crate::{
    db::Database,
    media::MediaStore,
    planner,
    providers::{language_request, provider_error_detail},
    repository::Repository,
    skills,
    value::{new_id, GENERATING, SUCCEEDED},
    worker::DurableWorker,
};

fn test_repository() -> (Repository, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let database = Database::open(root.join("ai_application_factory.db")).expect("test database");
    (Repository::new(database), root)
}

#[test]
fn language_response_errors_hide_raw_upstream_detail() {
    let error = crate::providers::language_response_read_error("error decoding response body");
    assert!(error.to_string().contains("内容格式无效"));
    assert!(!error.to_string().contains("error decoding response body"));
}

#[test]
fn language_transport_errors_are_translated() {
    let error = reqwest::blocking::Client::new()
        .get("https://[::1")
        .send()
        .expect_err("invalid URL must fail before a network request");
    let message = crate::providers::provider_transport_error("语言模型", error).to_string();
    assert!(message.contains("语言模型请求失败："));
    assert!(message.contains("请求发送失败"));
    assert!(!message.contains("invalid URL"));
}

#[test]
fn drama_project_task_and_local_media_round_trip() {
    let (repository, root) = test_repository();
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("本地短剧")),
            (
                "script".to_owned(),
                json!("林岩在旧站房找到泛黄信件，苏晚追上他，两人决定前往山村核实线索。"),
            ),
        ]))
        .expect("create project");
    let project_id = project["id"].as_str().expect("project id");
    let task_id = project["task_id"].as_str().expect("bootstrap task id");
    assert_eq!(project["task"]["status"], GENERATING);

    let plan = planner::fallback_drama_plan(
        "林岩在旧站房找到泛黄信件，苏晚追上他，两人决定前往山村核实线索。",
        "真人风格",
        "都市",
        80,
    );
    repository
        .save_drama_decomposition(project_id, &plan)
        .expect("save plan");
    let detail = repository.get_drama(project_id).expect("project detail");
    assert!(!detail["assets"].as_array().expect("assets").is_empty());
    let shot_id = detail["shots"][0]["id"].as_str().expect("shot id");

    let inserted = repository
        .create_shot(
            project_id,
            Map::from_iter([
                ("after_shot_id".to_owned(), json!(shot_id)),
                ("title".to_owned(), json!(detail["shots"][0]["title"])),
            ]),
        )
        .expect("insert same-title shot");
    assert_ne!(inserted["id"], shot_id);

    let first = repository
        .create_active_drama_task(project_id, "shot_prompt", Some(shot_id), Value::Null)
        .expect("first task");
    let duplicate = repository
        .create_active_drama_task(project_id, "shot_prompt", Some(shot_id), Value::Null)
        .expect("idempotent task");
    assert_eq!(first["id"], duplicate["id"]);
    assert!(repository.get_drama_task(task_id).is_ok());

    let media = MediaStore::new(repository).expect("media store");
    let url = media
        .save_data_url("data:image/png;base64,iVBORw0KGgo=")
        .expect("local upload");
    let id = url.trim_start_matches("/api/media/");
    assert!(media.path_for(id).is_some());
    assert!(media
        .provider_reference_url(&url)
        .is_some_and(|value| value.starts_with("data:image/png;base64,")));
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn storage_credentials_remain_visible_and_survive_blank_reprobe() {
    let (repository, root) = test_repository();
    let endpoint = json!("https://bucket.cos.ap-chengdu.myqcloud.com");
    let original = Map::from_iter([
        ("provider".to_owned(), json!("cos")),
        ("endpoint".to_owned(), endpoint.clone()),
        ("bucket".to_owned(), json!("bucket")),
        ("region".to_owned(), json!("ap-chengdu")),
        ("secret_id".to_owned(), json!("test-secret-id")),
        ("secret_key".to_owned(), json!("test-secret-key")),
    ]);
    repository
        .save_storage_config(original)
        .expect("save storage configuration");

    let public = repository.storage_config().expect("storage config");
    assert_eq!(public["secret_id"], "test-secret-id");
    assert_eq!(public["secret_key"], "test-secret-key");
    assert_eq!(public["secret_id_set"], true);
    assert_eq!(public["secret_key_set"], true);

    let blank_credential_form = Map::from_iter([
        ("provider".to_owned(), json!("cos")),
        ("endpoint".to_owned(), endpoint),
        ("bucket".to_owned(), json!("bucket")),
        ("region".to_owned(), json!("ap-chengdu")),
        ("secret_id".to_owned(), json!("")),
        ("secret_key".to_owned(), json!("")),
    ]);
    let candidate = repository
        .storage_config_candidate(&blank_credential_form)
        .expect("candidate preserves existing credentials");
    assert_eq!(candidate["secret_id"], "test-secret-id");
    assert_eq!(candidate["secret_key"], "test-secret-key");
    repository
        .save_storage_config(blank_credential_form)
        .expect("save blank credentials without clearing them");
    let saved = repository.storage_config().expect("saved storage config");
    assert_eq!(saved["secret_id"], "test-secret-id");
    assert_eq!(saved["secret_key"], "test-secret-key");
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn rich_prompt_generation_preserves_template_provenance_and_runs_quality() {
    let (repository, root) = test_repository();
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("富提示词短剧")),
            (
                "script".to_owned(),
                json!("林岩带着信件走进旧居，苏晚在门口追上他。"),
            ),
        ]))
        .expect("create project");
    let project_id = project["id"].as_str().expect("project id");
    let bootstrap = project["task_id"].as_str().expect("bootstrap task");
    let plan = planner::fallback_drama_plan(
        "林岩带着信件走进旧居，苏晚在门口追上他。",
        "真人风格",
        "都市",
        80,
    );
    repository
        .save_drama_decomposition(project_id, &plan)
        .expect("save plan");
    repository
        .finish_drama_task(bootstrap, SUCCEEDED, None, None)
        .expect("finish bootstrap");
    let detail = repository.get_drama(project_id).expect("detail");
    for asset in detail["assets"].as_array().expect("assets") {
        repository
            .mark_asset_succeeded(
                project_id,
                asset["id"].as_str().expect("asset id"),
                "/api/media/test-image",
            )
            .expect("ready image");
    }
    let shot_id = detail["shots"][0]["id"].as_str().expect("shot id");
    repository
        .set_shot_status(project_id, shot_id, GENERATING)
        .expect("mark prompt");
    repository
        .create_active_drama_task(
            project_id,
            "shot_prompt",
            Some(shot_id),
            json!({"shot_id":shot_id}),
        )
        .expect("prompt task");
    let media = MediaStore::new(repository.clone()).expect("media");
    let worker = DurableWorker::new(repository.clone(), media).expect("worker");
    assert!(worker.process_once().expect("prompt process"));
    let prompted = repository
        .get_shot(project_id, shot_id)
        .expect("prompted shot");
    assert!(prompted["prompt"]
        .as_str()
        .expect("prompt")
        .contains("【镜头1"));
    assert!(!prompted["prompt_template_version"]
        .as_str()
        .expect("template version")
        .is_empty());
    assert_eq!(prompted["quality_status"], "检查中");
    assert!(worker.process_once().expect("quality process"));
    let checked = repository
        .get_shot(project_id, shot_id)
        .expect("checked shot");
    assert!(checked["quality"].is_object());
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn game_graph_session_uses_persisted_nodes_and_edges() {
    let (repository, root) = test_repository();
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("本地互动游戏")),
            (
                "script".to_owned(),
                json!("玩家在废弃车站发现线索，需要在追踪陌生人或检查遗留行李之间做出选择。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    let plan = planner::fallback_game_plan(&game);
    repository
        .save_game_graph(
            game_id,
            plan["assets"].as_array().expect("assets"),
            plan["nodes"].as_array().expect("nodes"),
            plan["edges"].as_array().expect("edges"),
        )
        .expect("save graph");
    let session = repository
        .create_game_session(game_id)
        .expect("create session");
    assert_eq!(session["status"], "active");
    assert!(!session["choices"].as_array().expect("choices").is_empty());
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn legacy_sqlite_columns_and_model_storyboard_are_compatible() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    fs::create_dir_all(&root).expect("test directory");
    let path = root.join("legacy.db");
    rusqlite::Connection::open(&path)
        .expect("legacy database")
        .execute_batch("CREATE TABLE short_dramas (id TEXT PRIMARY KEY,name TEXT NOT NULL,script TEXT NOT NULL,ratio TEXT NOT NULL,style TEXT NOT NULL,theme TEXT NOT NULL,language_model TEXT NOT NULL,multimodal_model TEXT NOT NULL,status TEXT NOT NULL,shots_json TEXT NOT NULL DEFAULT '[]',assets_json TEXT NOT NULL DEFAULT '[]',historical_videos_json TEXT NOT NULL DEFAULT '[]',created_at TEXT NOT NULL,updated_at TEXT NOT NULL);")
        .expect("legacy schema");
    let database = Database::open(path).expect("migrate legacy schema");
    let columns = database
        .with_connection(|connection| {
            Ok(connection
                .prepare("PRAGMA table_info(short_dramas)")?
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?)
        })
        .expect("read migrated columns");
    assert!(columns.contains(&"expanded_script".to_owned()));
    assert!(columns.contains(&"video_model".to_owned()));

    let plan = planner::model_drama_plan(
        "```json\n{\"episodes\":[{\"name\":\"第1集\",\"shots\":[{\"title\":\"发现线索\",\"original_text\":\"林岩在站房找到信件。\",\"prompt\":\"中景跟拍人物发现信件\",\"duration_seconds\":6}]}],\"assets\":[{\"type\":\"prop\",\"name\":\"泛黄信件\",\"prompt\":\"旧纸张与蜡封特写\"}]}\n```",
        "林岩在站房找到信件。",
        "真人风格",
        "都市",
        80,
    )
    .expect("model plan");
    assert_eq!(plan["episodes"][0]["shots"][0]["title"], "发现线索");
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn rust_skill_envelopes_and_model_candidates_keep_python_provider_contracts() {
    let (repository, root) = test_repository();
    let skill = skills::drama_skill(
        "premise_expander",
        json!({
            "premise":"失忆的医生寻找真相",
            "genre":"悬疑",
            "target_audience":"短剧观众",
            "episode_count":25,
            "target_min_chars":5000,
            "target_max_chars":10000,
            "shot_script_max_chars":400,
        }),
    )
    .expect("premise skill");
    assert_eq!(skill["agent"], "drama");
    assert!(skill["instruction"]
        .as_str()
        .expect("instruction")
        .contains("5,000字"));

    let candidate = repository
        .model_config_candidate(&Map::from_iter([
            ("kind".to_owned(), json!("video")),
            ("provider".to_owned(), json!("dashscope")),
            ("api_key".to_owned(), json!("test-key")),
        ]))
        .expect("candidate");
    assert_eq!(candidate["model"], "wan2.6-r2v-flash");
    assert_eq!(
        candidate["query_url"],
        "https://dashscope.aliyuncs.com/api/v1/tasks/{id}"
    );
    assert_eq!(candidate["generation_concurrency"], 2);
    assert!(repository
        .setting("video")
        .expect("unset setting")
        .is_object());

    let tencent_image = repository
        .model_config_candidate(&Map::from_iter([
            ("kind".to_owned(), json!("multimodal")),
            ("provider".to_owned(), json!("tencent")),
            ("secret_id".to_owned(), json!("test-secret-id")),
            ("secret_key".to_owned(), json!("test-secret-key")),
        ]))
        .expect("Tencent MPS image candidate");
    assert_eq!(tencent_image["endpoint"], "https://mps.tencentcloudapi.com");
    assert_eq!(tencent_image["model"], "Hunyuan:3.0");
    assert_eq!(tencent_image["region"], "ap-guangzhou");
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn editor_banner_and_screenplay_endpoint_use_the_same_persisted_screenplay() {
    let (repository, root) = test_repository();
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("完整剧本预览")),
            (
                "script".to_owned(),
                json!("林砚在诊所发现旧信，决定追查真相。"),
            ),
        ]))
        .expect("create project");
    let project_id = project["id"].as_str().expect("project id");
    let task_id = project["task_id"].as_str().expect("task id");
    let screenplay = format!("开头{}结尾", "剧情推进。".repeat(1_100));
    repository
        .set_expanded_screenplay(project_id, &screenplay)
        .expect("save screenplay");
    repository
        .update_drama_task_snapshot(
            task_id,
            json!({"expanded_script_preview":"不应展示的任务快照"}),
        )
        .expect("save task preview");

    let detail = repository
        .get_editor_drama(project_id, None)
        .expect("editor detail");
    assert_eq!(detail["expanded_script"], screenplay);
    let endpoint = repository
        .get_expanded_screenplay(project_id)
        .expect("screenplay endpoint");
    assert_eq!(endpoint["expanded_script"], screenplay);
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn ark_plan_language_requests_use_chat_completions() {
    let (url, payload) = language_request(
        "ark",
        "https://ark.cn-beijing.volces.com/api/plan/v3",
        "doubao-seed-2.1-turbo",
        "system prompt",
        "user prompt",
        false,
    );

    assert_eq!(
        url,
        "https://ark.cn-beijing.volces.com/api/plan/v3/chat/completions"
    );
    assert_eq!(payload["messages"][0]["content"], "system prompt");
    assert_eq!(payload["messages"][1]["content"], "user prompt");
    assert!(payload.get("input").is_none());

    let (url, payload) = language_request(
        "ark",
        "https://ark.cn-beijing.volces.com/api/plan/v3/chat",
        "doubao-seed-2.1-turbo",
        "system prompt",
        "user prompt",
        false,
    );
    assert_eq!(
        url,
        "https://ark.cn-beijing.volces.com/api/plan/v3/chat/completions"
    );
    assert!(payload.get("messages").is_some());

    let (repository, root) = test_repository();
    let candidate = repository
        .model_config_candidate(&Map::from_iter([
            ("kind".to_owned(), json!("language")),
            ("provider".to_owned(), json!("ark")),
            (
                "endpoint".to_owned(),
                json!("https://ark.cn-beijing.volces.com/api/plan/v3/chat"),
            ),
            ("api_key".to_owned(), json!("test-key")),
        ]))
        .expect("candidate");
    assert_eq!(
        candidate["endpoint"],
        "https://ark.cn-beijing.volces.com/api/plan/v3/chat"
    );
    assert_eq!(candidate["model"], "doubao-seed-2.1-turbo");
    assert_eq!(candidate["models"][0], "doubao-seed-2.1-turbo");
    fs::remove_dir_all(root).expect("remove test data");

    let (url, payload) = language_request(
        "ark",
        "https://ark.cn-beijing.volces.com/api/v3",
        "doubao-seed-2.1-turbo",
        "system prompt",
        "user prompt",
        true,
    );
    assert_eq!(url, "https://ark.cn-beijing.volces.com/api/v3/responses");
    assert_eq!(payload["input"][1]["content"], "user prompt");
    assert_eq!(payload["tools"][0]["type"], "web_search");
}

#[test]
fn provider_errors_keep_the_upstream_model_compatibility_reason() {
    let detail = provider_error_detail(
        r#"{"error":{"code":"UnsupportedModel","message":"The requested model does not support the agent plan feature."}}"#,
    );
    assert_eq!(
        detail.as_deref(),
        Some("当前模型不支持此功能，请在设置中更换支持该功能的模型后重试。")
    );
}
