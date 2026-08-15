//! Regression coverage for the interactive-game expansion and DAG creation workflow.
use crate::{
    db::Database,
    planner,
    repository::Repository,
    value::{new_id, CANCELLED, FAILED, GENERATING, SUCCEEDED},
};
use serde_json::{json, Map};
use std::fs;

#[test]
fn game_creation_checkpoints_expansion_before_saving_a_playable_graph() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("钟楼回声")),
            (
                "script".to_owned(),
                json!("玩家在废弃钟楼收到失踪搭档的录音，需要在警报响起前找出真相。"),
            ),
            ("success_ending_count".to_owned(), json!(1)),
            ("failure_ending_count".to_owned(), json!(1)),
            ("branch_min".to_owned(), json!(2)),
            ("branch_max".to_owned(), json!(2)),
            ("resolution".to_owned(), json!("480p")),
            ("enable_web_search".to_owned(), json!(false)),
            ("expanded_script_min_chars".to_owned(), json!(100)),
            ("expanded_script_max_chars".to_owned(), json!(200)),
            ("node_script_max_chars".to_owned(), json!(60)),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    assert_eq!(game["task"]["type"], "game_script_expansion");
    repository
        .complete_game_screenplay_expansion(
            game["task"]["id"].as_str().expect("expansion task"),
            game_id,
            "【剧情段 S01｜开始】\n剧情正文：钟楼警报响起。\n【玩家抉择】\n【结局 E01｜成功】\n【结局 E02｜失败】",
            game["script"].as_str().expect("script").chars().count(),
            true,
        )
        .expect("checkpoint expansion");
    let expanded = repository.get_game(game_id).expect("expanded game");
    assert!(expanded["expanded_script"]
        .as_str()
        .unwrap()
        .contains("钟楼"));
    assert_eq!(expanded["resolution"], "480p");
    assert!(expanded["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .any(|task| task["type"] == "game_graph_decomposition" && task["status"] == GENERATING));
    let plan = planner::fallback_game_plan(&expanded);
    repository
        .save_game_graph(
            game_id,
            plan["assets"].as_array().expect("assets"),
            plan["nodes"].as_array().expect("nodes"),
            plan["edges"].as_array().expect("edges"),
        )
        .expect("save graph fixture");
    let planned = repository.get_game(game_id).expect("planned game");
    assert_eq!(planned["status"], SUCCEEDED);
    let nodes = planned["nodes"].as_array().expect("nodes");
    let edges = planned["edges"].as_array().expect("edges");
    assert!(nodes.len() >= 3);
    assert!(edges.len() >= 2);
    let terminal_counts = ["success", "failure"].map(|kind| {
        nodes
            .iter()
            .filter(|node| node["node_type"] == kind)
            .count()
    });
    assert_eq!(terminal_counts, [1, 1]);
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn game_editor_save_persists_the_toolbar_title() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("旧标题")),
            (
                "script".to_owned(),
                json!("玩家在钟楼听见一段陌生录音，并在两个线索之间做出选择。"),
            ),
        ]))
        .expect("create game");
    assert_eq!(game["failure_ending_count"], 12);
    let game_id = game["id"].as_str().expect("game id");
    repository
        .save_game_graph(
            game_id,
            &[json!({"id":"hero","type":"character","name":"钟楼守夜人","prompt":"守夜人"})],
            &[json!({"id":"start","node_type":"start","title":"钟楼入口","original_text":"走进钟楼","prompt":"钟楼入口","duration_seconds":10}), json!({"id":"ending","node_type":"success","title":"真相揭晓","original_text":"找到真相","prompt":"真相揭晓","duration_seconds":10})],
            &[json!({"id":"choice","source_node_id":"start","target_node_id":"ending","option_text":"进入钟楼","sort_order":1})],
        )
        .expect("save game graph");
    let updated = repository
        .save_game_editor(
            game_id,
            Map::from_iter([("name".to_owned(), json!("钟楼回声"))]),
        )
        .expect("save game editor");
    assert_eq!(updated["name"], "钟楼回声");
    assert_eq!(
        repository.get_game(game_id).expect("load game")["name"],
        "钟楼回声"
    );
    let snapshot: (String, String, String) = repository
        .db
        .with_connection(|connection| {
            Ok(connection.query_row(
                "SELECT assets_json,nodes_json,edges_json FROM interactive_games WHERE id=?1",
                [game_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?)
        })
        .expect("load graph snapshot");
    assert!(snapshot.0.contains("钟楼守夜人"));
    assert!(snapshot.1.contains("钟楼入口"));
    assert!(snapshot.2.contains("进入钟楼"));
    fs::remove_dir_all(root).expect("remove test data");
}
#[test]
fn game_editor_save_keeps_structure_without_validating_model_choices() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("旧标题")),
            (
                "script".to_owned(),
                json!("玩家在钟楼听见一段陌生录音，并在两个线索之间做出选择。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    repository
        .save_game_graph(
            game_id,
            &[],
            &[json!({"id":"start","node_type":"start","title":"钟楼入口","original_text":"走进钟楼","prompt":"钟楼入口","duration_seconds":10})],
            &[],
        )
        .expect("save game graph");
    let saved = repository
        .save_game_editor(
            game_id,
            Map::from_iter([
                ("name".to_owned(), json!("钟楼回声")),
                ("language_model".to_owned(), json!("stale-model")),
                ("multimodal_model".to_owned(), json!("stale-model")),
                ("video_model".to_owned(), json!("stale-model")),
            ]),
        )
        .expect("save game structure without probing models");
    assert_eq!(saved["name"], "钟楼回声");
    assert_eq!(saved["nodes"].as_array().expect("nodes").len(), 1);
    fs::remove_dir_all(root).expect("remove test data");
}
#[test]
fn game_global_parameters_and_screenplay_updates_are_persisted() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("钟楼回声")),
            (
                "script".to_owned(),
                json!("玩家在钟楼听见一段陌生录音，并在两个线索之间做出选择。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    let parameters = repository
        .update_game_parameters(
            game_id,
            Map::from_iter([
                ("style".to_owned(), json!("2D动漫")),
                ("language_model".to_owned(), json!("language-v2")),
                ("multimodal_model".to_owned(), json!("image-v2")),
                ("video_model".to_owned(), json!("video-v2")),
                ("enable_web_search".to_owned(), json!(true)),
            ]),
        )
        .expect("save global parameters");
    assert_eq!(parameters["style"], "2D动漫");
    assert_eq!(parameters["language_model"], "language-v2");
    assert_eq!(parameters["enable_web_search"], true);
    let screenplay = repository
        .update_game_screenplay(
            game_id,
            Map::from_iter([
                (
                    "script".to_owned(),
                    json!("玩家在钟楼听见一段陌生录音，并在两个线索之间做出选择，寻找失踪的同伴。"),
                ),
                (
                    "expanded_script".to_owned(),
                    json!("扩写后的互动游戏正文。"),
                ),
            ]),
        )
        .expect("save screenplay");
    assert_eq!(screenplay["expanded_script"], "扩写后的互动游戏正文。");
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn game_generation_preview_snapshot_is_visible_to_the_editor() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("钟楼回声")),
            (
                "script".to_owned(),
                json!("玩家在钟楼听见一段陌生录音，并在两个线索之间做出选择。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    let task_id = game["task"]["id"].as_str().expect("task id");
    repository
        .update_game_task_snapshot(
            task_id,
            json!({"graph_preview":"实时游戏图谱片段","preview_received_chars":42,"game_id":game_id}),
        )
        .expect("persist preview");
    let saved_game = repository.get_game(game_id).expect("load game");
    let task = saved_game["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .find(|item| item["id"] == task_id)
        .expect("preview task");
    assert_eq!(task["input_snapshot"]["preview_received_chars"], 42);
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn game_screenplay_expansion_can_stop_and_restart_without_task_overwrite() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("钟楼回声")),
            (
                "script".to_owned(),
                json!("玩家在钟楼听见一段陌生录音，并在两个线索之间做出选择。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    let cancelled_id = game["task"]["id"].as_str().expect("task id");
    let cancelled = repository
        .cancel_game_screenplay(game_id)
        .expect("stop screenplay");
    assert_eq!(cancelled["status"], CANCELLED);
    let retained = repository
        .finish_game_task(cancelled_id, SUCCEEDED, None, None)
        .expect("ignore stale worker completion");
    assert_eq!(retained["status"], CANCELLED);

    let restarted = repository
        .continue_game_screenplay(game_id)
        .expect("restart screenplay");
    assert_ne!(restarted["id"], cancelled_id);
    assert_eq!(restarted["type"], "game_script_expansion");
    assert_eq!(restarted["status"], GENERATING);
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn failed_game_generation_retries_from_its_saved_checkpoint() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("断点重试")),
            (
                "script".to_owned(),
                json!("玩家在钟楼听见一段陌生录音，并在两个线索之间做出选择。"),
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
    repository
        .finish_game_task(task_id, FAILED, None, Some("语言模型暂时不可用"))
        .expect("mark failure");

    let retried = repository
        .retry_game_generation(game_id)
        .expect("retry generation");
    assert_eq!(retried["id"], task_id);
    assert_eq!(retried["status"], GENERATING);
    assert_eq!(
        retried["input_snapshot"]["expanded_script_preview"],
        "已保存的扩写片段"
    );
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn game_material_decomposition_keeps_manual_reference_and_frame_configuration() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("钟楼回声")),
            (
                "script".to_owned(),
                json!("玩家在钟楼听见一段陌生录音，并在两个线索之间做出选择。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    repository
        .save_game_graph(
            game_id,
            &[
                json!({"id":"hero","type":"character","name":"林砚","prompt":"叙述背景主题：互动游戏\n青年调查员，冷静观察环境，危机中优先保护同伴。"}),
                json!({"id":"tower","type":"scene","name":"废弃钟楼","prompt":"叙述背景主题：互动游戏\n老旧石塔内部，旋梯、破碎钟面和冷色警报灯。"}),
                json!({"id":"recording","type":"prop","name":"失踪搭档录音机","prompt":"叙述背景主题：互动游戏\n磨损金属录音机，闪烁的红色录制灯。"}),
            ],
            &[json!({"id":"start","node_type":"start","title":"钟楼入口","original_text":"林砚拿起录音机。","prompt":"场景：@图1\n角色：@图2\n道具：@图3","reference_asset_ids":["hero","tower","recording"],"duration_seconds":10})],
            &[],
        )
        .expect("save graph");
    let graph = repository.get_game(game_id).expect("load graph");
    let assets = graph["assets"].as_array().expect("assets");
    assert!(assets.iter().any(|asset| asset["type"] == "character"
        && asset["prompt"]
            .as_str()
            .unwrap_or_default()
            .starts_with("叙述背景主题：")));
    assert!(assets.iter().any(|asset| asset["type"] == "scene"));
    assert!(assets.iter().any(|asset| asset["type"] == "prop"));
    let placeholder = assets
        .iter()
        .find(|asset| asset["type"] == "placeholder")
        .expect("placeholder");
    assert!(assets.iter().any(|asset| asset["type"] == "cover"));
    let hero = assets
        .iter()
        .find(|asset| asset["name"] == "林砚")
        .expect("hero");
    let tower = assets
        .iter()
        .find(|asset| asset["name"] == "废弃钟楼")
        .expect("tower");
    let recording = assets
        .iter()
        .find(|asset| asset["name"] == "失踪搭档录音机")
        .expect("recording");
    let node_id = graph["nodes"][0]["id"].as_str().expect("node id");
    let node = repository
        .update_game_node(
            game_id,
            node_id,
            Map::from_iter([
                (
                    "reference_asset_ids".to_owned(),
                    json!([hero["id"], tower["id"], recording["id"]]),
                ),
                (
                    "first_last_frames".to_owned(),
                    json!({"first":{"asset_id":hero["id"]},"last":{"asset_id":tower["id"]}}),
                ),
                ("placeholder_asset_id".to_owned(), json!(placeholder["id"])),
            ]),
        )
        .expect("save node media configuration");
    assert_eq!(
        node["reference_asset_ids"]
            .as_array()
            .expect("references")
            .len(),
        3
    );
    assert_eq!(node["first_last_frames"]["first"]["asset_id"], hero["id"]);
    assert_eq!(node["first_last_frames"]["last"]["asset_id"], tower["id"]);
    assert_eq!(node["placeholder_asset_id"], placeholder["id"]);
    repository
        .update_game_asset(
            game_id,
            hero["id"].as_str().expect("hero id"),
            Map::from_iter([(
                "image_url".to_owned(),
                json!("https://example.com/hero.png"),
            )]),
        )
        .expect("configure manual reference image");
    let task = repository
        .enqueue_game_node_video(game_id, node_id)
        .expect("queue node video");
    assert_eq!(
        repository
            .get_game_node(game_id, node_id)
            .expect("queued node")["status"],
        GENERATING
    );
    let generated_prompt = task["input_snapshot"]["prompt"]
        .as_str()
        .expect("generated prompt");
    assert!(["场景：@图", "角色：@图", "道具：@图"]
        .iter()
        .all(|label| generated_prompt.contains(label)));
    assert_eq!(
        task["input_snapshot"]["reference_images"]
            .as_array()
            .expect("reference images")
            .len(),
        3
    );
    assert_eq!(
        task["input_snapshot"]["first_last_frames"]["first"]["asset_id"],
        hero["id"]
    );
    fs::remove_dir_all(root).expect("remove test data");
}
