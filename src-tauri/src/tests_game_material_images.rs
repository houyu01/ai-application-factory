//! Regression coverage for interactive-game material image tasks, public prompts, and alternate forms.

use std::fs;

use serde_json::{json, Map};

use crate::{
    db::Database,
    repository::Repository,
    value::{new_id, GENERATING, SUCCEEDED},
};

#[test]
fn game_material_images_keep_durable_history_and_variant_state() {
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
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    repository
        .save_game_graph(
            game_id,
            &[json!({"id":"hero","type":"character","name":"守夜人","prompt":"黑色风衣的钟楼守夜人"})],
            &[],
            &[],
        )
        .expect("save assets");
    let asset_id = repository.get_game(game_id).expect("game")["assets"]
        .as_array()
        .expect("assets")
        .iter()
        .find(|asset| asset["type"] == "character")
        .and_then(|asset| asset["id"].as_str())
        .expect("character")
        .to_owned();

    let updated = repository
        .update_game_asset_public_prompt(
            game_id,
            Map::from_iter([
                ("asset_type".to_owned(), json!("character")),
                (
                    "public_prompt".to_owned(),
                    json!("保持正侧背三视图和身份一致。"),
                ),
            ]),
        )
        .expect("save public prompt");
    assert_eq!(
        updated["asset_public_prompts"]["character"],
        "保持正侧背三视图和身份一致。"
    );

    let task = repository
        .enqueue_game_asset_image(game_id, &asset_id)
        .expect("enqueue base image");
    assert_eq!(task["type"], "game_asset_image");
    assert_eq!(task["status"], GENERATING);
    repository
        .finish_game_asset_image(
            game_id,
            &asset_id,
            task["id"].as_str().unwrap(),
            "media://base",
        )
        .expect("save image");
    repository
        .finish_game_task(task["id"].as_str().unwrap(), SUCCEEDED, None, None)
        .expect("finish base task");

    let variant = repository
        .create_game_asset_variant(
            game_id,
            &asset_id,
            Map::from_iter([
                ("name".to_owned(), json!("雨夜形态")),
                (
                    "prompt".to_owned(),
                    json!("保持身份一致，黑色雨衣与潮湿光泽。"),
                ),
            ]),
        )
        .expect("create variant");
    let variant_id = variant["id"].as_str().expect("variant id");
    let variant_task = repository
        .enqueue_game_asset_variant_image(game_id, &asset_id, variant_id)
        .expect("enqueue variant image");
    repository
        .finish_game_asset_variant_image(
            game_id,
            &asset_id,
            variant_id,
            variant_task["id"].as_str().unwrap(),
            "media://variant",
        )
        .expect("save variant image");
    repository
        .finish_game_task(variant_task["id"].as_str().unwrap(), SUCCEEDED, None, None)
        .expect("finish variant task");

    let updated_game = repository.get_game(game_id).expect("updated game");
    let asset = updated_game["assets"]
        .as_array()
        .expect("assets")
        .iter()
        .find(|asset| asset["id"] == asset_id)
        .expect("updated asset");
    assert_eq!(asset["image_url"], "media://base");
    assert_eq!(asset["image_history"].as_array().unwrap().len(), 1);
    assert_eq!(asset["variants"][0]["image_url"], "media://variant");
    assert_eq!(
        asset["variants"][0]["image_history"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn game_node_reference_images_only_enqueue_missing_materials_and_reuse_active_tasks() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("钟楼回声")),
            (
                "script".to_owned(),
                json!("玩家在雨夜钟楼追踪失踪搭档，需要决定是否相信一段来源不明的录音。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    repository
        .save_game_graph(
            game_id,
            &[
                json!({"id":"hero","type":"character","name":"守夜人","prompt":"黑色风衣的钟楼守夜人"}),
                json!({"id":"tower","type":"scene","name":"钟楼顶层","prompt":"雨夜的钟楼顶层"}),
            ],
            &[json!({"id":"opening","title":"抵达钟楼","original_text":"守夜人抵达钟楼顶层。","reference_asset_ids":["hero","tower"]})],
            &[],
        )
        .expect("save graph");
    let saved = repository.get_game(game_id).expect("load game");
    let node_id = saved["nodes"][0]["id"].as_str().expect("node id");
    let hero_id = saved["assets"]
        .as_array()
        .expect("assets")
        .iter()
        .find(|asset| asset["type"] == "character")
        .and_then(|asset| asset["id"].as_str())
        .expect("hero id");
    let tower_id = saved["assets"]
        .as_array()
        .expect("assets")
        .iter()
        .find(|asset| asset["type"] == "scene")
        .and_then(|asset| asset["id"].as_str())
        .expect("tower id");
    repository
        .update_game_asset(
            game_id,
            tower_id,
            Map::from_iter([("image_url".to_owned(), json!("media://tower"))]),
        )
        .expect("configure scene image");

    let queued = repository
        .enqueue_game_node_reference_images(game_id, node_id)
        .expect("queue missing reference");
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0]["type"], "game_asset_image");
    assert_eq!(queued[0]["resource_id"], hero_id);

    let retried = repository
        .enqueue_game_node_reference_images(game_id, node_id)
        .expect("reuse active task");
    assert_eq!(retried[0]["id"], queued[0]["id"]);

    repository
        .finish_game_asset_image(
            game_id,
            hero_id,
            queued[0]["id"].as_str().expect("task id"),
            "media://hero",
        )
        .expect("save generated image");
    repository
        .finish_game_task(
            queued[0]["id"].as_str().expect("task id"),
            SUCCEEDED,
            None,
            None,
        )
        .expect("finish task");
    let error = repository
        .enqueue_game_node_reference_images(game_id, node_id)
        .expect_err("ready references do not queue new tasks");
    assert!(error.to_string().contains("均已就绪"));
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn game_cover_tasks_retain_reference_groups_and_every_requested_image() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("雾港抉择")),
            (
                "script".to_owned(),
                json!("玩家在雾港接到失踪同伴的求救信号，需要在码头与旧城区之间做出选择。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    repository
        .save_game_graph(
            game_id,
            &[
                json!({"id":"hero","type":"character","name":"调查员","prompt":"雨夜调查员"}),
                json!({"id":"pier","type":"scene","name":"雾港码头","prompt":"浓雾码头"}),
            ],
            &[],
            &[],
        )
        .expect("save graph");
    let saved = repository.get_game(game_id).expect("load game");
    let character_id = saved["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|asset| asset["type"] == "character")
        .and_then(|asset| asset["id"].as_str())
        .expect("character");
    let scene_id = saved["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|asset| asset["type"] == "scene")
        .and_then(|asset| asset["id"].as_str())
        .expect("scene");
    repository
        .update_game_asset(
            game_id,
            character_id,
            Map::from_iter([(
                "image_url".to_owned(),
                json!("https://example.com/hero.png"),
            )]),
        )
        .expect("save character image");
    repository
        .update_game_asset(
            game_id,
            scene_id,
            Map::from_iter([(
                "image_url".to_owned(),
                json!("https://example.com/pier.png"),
            )]),
        )
        .expect("save scene image");
    let extra = repository
        .create_game_cover_reference(game_id, "海报构图", "https://example.com/layout.png")
        .expect("save extra reference");
    let queued = repository.enqueue_game_cover(game_id, "雾港抉择", "突出雨夜对峙与分支抉择。", json!({"ratio":"16:9","count":2,"character_asset_ids":[character_id],"scene_asset_ids":[scene_id],"extra_reference_asset_ids":[extra["id"]],"reference_asset_ids":[character_id,scene_id,extra["id"]]})).expect("enqueue cover");

    assert_eq!(queued["task"]["type"], "game_cover_image");
    assert_eq!(queued["cover"]["metadata"]["count"], 2);
    let cover_id = queued["cover"]["id"].as_str().expect("cover id");
    let task_id = queued["task"]["id"].as_str().expect("task id");
    repository
        .finish_game_asset_image(game_id, cover_id, task_id, "media://cover-1")
        .expect("first cover");
    repository
        .finish_game_asset_image(game_id, cover_id, task_id, "media://cover-2")
        .expect("second cover");
    repository
        .finish_game_task(task_id, SUCCEEDED, None, None)
        .expect("finish cover task");

    let cover = repository
        .get_game_asset(game_id, cover_id)
        .expect("load cover");
    assert_eq!(cover["image_history"].as_array().unwrap().len(), 2);
    assert_eq!(
        cover["metadata"]["reference_asset_ids"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn game_placeholder_layouts_keep_composite_history_and_bind_the_completed_node_image() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let repository = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("test database"),
    );
    let game = repository
        .create_game(Map::from_iter([
            ("name".to_owned(), json!("雾港抉择")),
            (
                "script".to_owned(),
                json!("玩家在雾港码头寻找失踪同伴，需要在暴雨到来前作出选择。"),
            ),
        ]))
        .expect("create game");
    let game_id = game["id"].as_str().expect("game id");
    repository
        .save_game_graph(
            game_id,
            &[
                json!({"id":"hero","type":"character","name":"调查员","prompt":"雨夜调查员"}),
                json!({"id":"pier","type":"scene","name":"雾港码头","prompt":"浓雾码头"}),
            ],
            &[json!({"id":"opening","title":"抵达码头","original_text":"调查员抵达码头。","prompt":"调查员在雾港码头观察线索。","duration_seconds":10})],
            &[],
        )
        .expect("save graph");
    let saved = repository.get_game(game_id).expect("load game");
    let assets = saved["assets"].as_array().expect("assets");
    let character_id = assets
        .iter()
        .find(|asset| asset["type"] == "character")
        .and_then(|asset| asset["id"].as_str())
        .expect("character");
    let scene_id = assets
        .iter()
        .find(|asset| asset["type"] == "scene")
        .and_then(|asset| asset["id"].as_str())
        .expect("scene");
    let node_id = saved["nodes"][0]["id"].as_str().expect("node");
    for (id, url) in [(character_id, "media://hero"), (scene_id, "media://scene")] {
        repository
            .update_game_asset(
                game_id,
                id,
                Map::from_iter([("image_url".to_owned(), json!(url))]),
            )
            .expect("configure image");
    }
    let layout = repository
        .save_game_placeholder_layout(
            game_id,
            node_id,
            Map::from_iter([
                ("scene_asset_id".to_owned(), json!(scene_id)),
                (
                    "placements".to_owned(),
                    json!([{"id":"hero-placement","asset_id":character_id,"x":0.2,"y":0.3,"width":0.25,"height":0.4,"note":"手持手电"}]),
                ),
            ]),
        )
        .expect("save layout");
    assert_eq!(layout["placeholder_scene_asset_id"], scene_id);
    assert_eq!(layout["placeholder_placements"][0]["note"], "手持手电");

    let queued = repository
        .enqueue_game_placeholder(
            game_id,
            node_id,
            "生成干净的雾港码头构图参考图。",
            json!({"node_id":node_id,"scene_asset_id":scene_id,"placements":layout["placeholder_placements"],"character_asset_ids":[character_id],"prop_asset_ids":[],"reference_asset_ids":[scene_id,character_id]}),
        )
        .expect("enqueue placeholder");
    let placeholder_id = queued["placeholder"]["id"]
        .as_str()
        .expect("placeholder id");
    let task_id = queued["task"]["id"].as_str().expect("task id");
    assert_eq!(queued["task"]["type"], "game_placeholder_image");
    assert_eq!(queued["placeholder"]["metadata"]["version"], 1);
    repository
        .finish_game_asset_image(game_id, placeholder_id, task_id, "media://placeholder")
        .expect("save placeholder image");
    repository
        .apply_game_placeholder_to_node(
            game_id,
            node_id,
            placeholder_id,
            &queued["placeholder"]["metadata"],
        )
        .expect("bind node placeholder");
    repository
        .finish_game_task(task_id, SUCCEEDED, None, None)
        .expect("finish placeholder task");

    let node = repository
        .get_game_node(game_id, node_id)
        .expect("load node");
    let placeholder = repository
        .get_game_asset(game_id, placeholder_id)
        .expect("load placeholder");
    assert_eq!(node["placeholder_asset_id"], placeholder_id);
    assert_eq!(node["placeholder_scene_asset_id"], scene_id);
    assert_eq!(node["placeholder_placements"][0]["asset_id"], character_id);
    assert_eq!(placeholder["image_history"].as_array().unwrap().len(), 1);
    fs::remove_dir_all(root).expect("remove test data");
}
