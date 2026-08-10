//! Regression coverage for asset voice-preset persistence boundaries.

use std::fs;

use serde_json::{json, Map};

use crate::{db::Database, repository::Repository, value::new_id};

fn repository() -> (Repository, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let database = Database::open(root.join("ai_application_factory.db")).expect("test database");
    (Repository::new(database), root)
}

#[test]
fn asset_voice_ids_must_be_enabled_presets_and_can_be_cleared() {
    let (repository, root) = repository();
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("音色校验短剧")),
            (
                "script".to_owned(),
                json!("主角在雨夜收到旧信，决定前往车站追查寄信人的身份。"),
            ),
        ]))
        .expect("create project");
    let project_id = project["id"].as_str().expect("project id");

    let error = repository
        .create_asset(
            project_id,
            Map::from_iter([
                ("type".to_owned(), json!("character")),
                ("name".to_owned(), json!("林岩")),
                ("voice_id".to_owned(), json!("missing-voice")),
            ]),
        )
        .expect_err("unknown voice must be rejected");
    assert_eq!(error.to_string(), "Voice preset not found: missing-voice");

    let voice_id = repository
        .voices()
        .expect("voice catalog")
        .into_iter()
        .find(|voice| voice["id"].as_str() != Some("none"))
        .and_then(|voice| voice["id"].as_str().map(str::to_owned))
        .expect("enabled voice id");
    let asset = repository
        .create_asset(
            project_id,
            Map::from_iter([
                ("type".to_owned(), json!("character")),
                ("name".to_owned(), json!("林岩")),
                ("voice_id".to_owned(), json!(voice_id)),
            ]),
        )
        .expect("valid voice");
    let asset_id = asset["id"].as_str().expect("asset id");

    assert!(repository
        .update_asset(
            project_id,
            asset_id,
            Map::from_iter([("voice_id".to_owned(), json!("missing-voice"))]),
        )
        .is_err());
    assert_eq!(
        repository.get_asset(project_id, asset_id).expect("asset")["voice_id"],
        voice_id
    );

    let cleared = repository
        .update_asset(
            project_id,
            asset_id,
            Map::from_iter([("voice_id".to_owned(), serde_json::Value::Null)]),
        )
        .expect("clear voice");
    assert!(cleared["voice_id"].is_null());
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn creator_voice_preset_is_persisted_and_can_be_assigned_to_a_character() {
    let (repository, root) = repository();
    let preset = repository
        .create_voice_preset(Map::from_iter([
            ("name".to_owned(), json!("知性纪录片旁白")),
            ("gender".to_owned(), json!("女")),
            (
                "prompt".to_owned(),
                json!("沉静清晰的成年女声，语速平稳，适合讲述与纪录片旁白。"),
            ),
        ]))
        .expect("create custom voice");
    let voice_id = preset["id"].as_str().expect("custom voice id");
    assert!(voice_id.starts_with("custom-"));
    assert!(repository
        .voices()
        .expect("voice catalog")
        .iter()
        .any(|voice| voice["id"] == voice_id && voice["name"] == "知性纪录片旁白"));

    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("自定义音色短剧")),
            (
                "script".to_owned(),
                json!("旁白在晨雾中讲述一位旅人回到故乡寻找旧友的故事。"),
            ),
        ]))
        .expect("create project");
    let asset = repository
        .create_asset(
            project["id"].as_str().expect("project id"),
            Map::from_iter([
                ("type".to_owned(), json!("character")),
                ("name".to_owned(), json!("旁白")),
                ("voice_id".to_owned(), json!(voice_id)),
            ]),
        )
        .expect("assign custom voice");
    assert_eq!(asset["voice_id"], voice_id);

    let duplicate = repository
        .create_voice_preset(Map::from_iter([
            ("name".to_owned(), json!("知性纪录片旁白")),
            ("prompt".to_owned(), json!("另一段描述")),
        ]))
        .expect_err("duplicate name is rejected");
    assert_eq!(duplicate.to_string(), "已存在同名音色，请修改名称后再保存");
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn decomposition_rejects_an_unknown_character_voice_id() {
    let (repository, root) = repository();
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("拆解音色校验")),
            (
                "script".to_owned(),
                json!("林岩在旧车站找到线索并决定继续调查。"),
            ),
        ]))
        .expect("create project");
    let project_id = project["id"].as_str().expect("project id");
    let plan = json!({
        "episodes": [],
        "assets": [{
            "id": "character",
            "type": "character",
            "name": "林岩",
            "prompt": "青年男性角色",
            "voice_id": "missing-voice"
        }]
    });

    let error = repository
        .save_drama_decomposition(project_id, &plan)
        .expect_err("unknown planned voice must be rejected");
    assert_eq!(error.to_string(), "Voice preset not found: missing-voice");
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn child_and_adult_character_forms_persist_and_target_their_own_shots() {
    let (repository, root) = repository();
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("成长剧本")),
            (
                "script".to_owned(),
                json!("林砚幼年在山村练剑，多年后以成年剑修身份归来。"),
            ),
        ]))
        .expect("create project");
    let project_id = project["id"].as_str().expect("project id");
    let plan = json!({"episodes":[{"name":"第1集","shots":[
        {"title":"幼年练剑","original_text":"八岁的林砚在山村练剑。","references":[{"asset_type":"character","asset_name":"林砚","variant_name":"幼年形态"}]},
        {"title":"成年归来","original_text":"成年林砚归来。","references":[{"asset_type":"character","asset_name":"林砚"}]}
    ]}],"assets":[{"id":"lin-yan","type":"character","name":"林砚","prompt":"成年剑修，克制坚定。","voice_id":"warm_older_brother_male","variants":[{"id":"child","name":"幼年形态","prompt":"八岁，圆脸短发，粗布短褂。","episode_numbers":[1]}]}]});
    repository
        .save_drama_decomposition(project_id, &plan)
        .expect("save decomposition");
    let detail = repository.get_drama(project_id).expect("detail");
    let character = &detail["assets"][0];
    let child_id = character["variants"][0]["id"]
        .as_str()
        .expect("child form id");
    assert_eq!(character["variants"][0]["name"], "幼年形态");
    let child_shot = &detail["shots"][0];
    let adult_shot = &detail["shots"][1];
    assert!(child_shot["prompt_rich"]
        .as_array()
        .is_some_and(|nodes| nodes.iter().any(|node| node["variant_id"] == child_id)));
    assert!(adult_shot["prompt_rich"]
        .as_array()
        .is_some_and(|nodes| nodes.iter().all(|node| node["variant_id"].is_null())));
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn saving_rich_prompt_references_keeps_the_shot_reference_list_in_sync() {
    let (repository, root) = repository();
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("引用同步")),
            ("script".to_owned(), json!("林岩在旧居找到信物。")),
        ]))
        .expect("create project");
    let project_id = project["id"].as_str().expect("project id");
    repository
        .save_drama_decomposition(project_id, &json!({"episodes":[{"name":"第1集","shots":[{"title":"分镜","original_text":"林岩在旧居找到信物。"}]}],"assets":[]}))
        .expect("save decomposition");
    let shot_id = repository.get_drama(project_id).expect("detail")["shots"][0]["id"]
        .as_str()
        .expect("shot id")
        .to_owned();
    repository
        .update_shot(project_id, &shot_id, Map::from_iter([("prompt_rich".to_owned(), json!([
            {"type":"reference","asset_id":"scene","asset_type":"scene","label":"旧居"},
            {"type":"reference","asset_id":"character","asset_type":"character","label":"林岩"},
            {"type":"reference","asset_id":"scene","asset_type":"scene","label":"旧居"}
        ]))]))
        .expect("save rich prompt");

    assert_eq!(
        repository.get_shot(project_id, &shot_id).expect("shot")["reference_asset_ids"],
        json!(["scene", "character"])
    );
    fs::remove_dir_all(root).expect("remove test data");
}
