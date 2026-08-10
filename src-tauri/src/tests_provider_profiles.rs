//! Regression coverage for switching between saved model-provider settings cards.

use std::fs;

use serde_json::{json, Map};

use crate::{db::Database, repository::Repository, value::new_id};

fn test_repository() -> (Repository, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("provider-profile-{}", new_id()));
    let database = Database::open(root.join("settings.db")).expect("test database");
    (Repository::new(database), root)
}

#[test]
fn saving_another_provider_keeps_the_previous_api_key_and_public_profile() {
    let (repository, root) = test_repository();
    repository
        .save_model_config(Map::from_iter([
            ("kind".to_owned(), json!("video")),
            ("provider".to_owned(), json!("ark")),
            ("api_key".to_owned(), json!("ark-key")),
            ("model".to_owned(), json!("ark-custom")),
            ("models".to_owned(), json!(["ark-custom"])),
        ]))
        .expect("save Ark profile");
    repository
        .save_model_config(Map::from_iter([
            ("kind".to_owned(), json!("video")),
            ("provider".to_owned(), json!("dashscope")),
            ("api_key".to_owned(), json!("dashscope-key")),
            ("model".to_owned(), json!("wan-custom")),
            ("models".to_owned(), json!(["wan-custom"])),
        ]))
        .expect("save DashScope profile");

    let stored = repository.setting("video").expect("stored video setting");
    assert_eq!(stored["provider"], "dashscope");
    assert_eq!(stored["provider_profiles"]["ark"]["api_key"], "ark-key");
    assert_eq!(
        stored["provider_profiles"]["dashscope"]["api_key"],
        "dashscope-key"
    );
    let public = repository.model_configs().expect("public settings");
    assert_eq!(
        public["video"]["provider_profiles"]["ark"]["model"],
        "ark-custom"
    );
    assert_eq!(
        public["video"]["provider_profiles"]["ark"]["api_key_set"],
        true
    );
    assert!(public["video"]["provider_profiles"]["ark"]
        .get("api_key")
        .is_none());
    assert_eq!(
        repository
            .model_api_key("video", Some("ark"))
            .expect("Ark key")["api_key"],
        "ark-key"
    );
    assert_eq!(
        repository
            .model_api_key("video", Some("dashscope"))
            .expect("DashScope key")["api_key"],
        "dashscope-key"
    );
    assert!(repository.model_api_key("video", Some("tencent")).is_err());
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn model_option_edits_stay_with_the_requested_provider() {
    let (repository, root) = test_repository();
    repository
        .save_model_config(Map::from_iter([
            ("kind".to_owned(), json!("multimodal")),
            ("provider".to_owned(), json!("ark")),
            ("api_key".to_owned(), json!("ark-key")),
            ("models".to_owned(), json!(["ark-image"])),
            ("model".to_owned(), json!("ark-image")),
        ]))
        .expect("save Ark profile");
    repository
        .save_model_options(
            "multimodal",
            Map::from_iter([
                ("provider".to_owned(), json!("dashscope")),
                ("models".to_owned(), json!(["qwen-image-custom"])),
                ("model".to_owned(), json!("qwen-image-custom")),
            ]),
        )
        .expect("save DashScope model choices");

    let stored = repository
        .setting("multimodal")
        .expect("stored image setting");
    assert_eq!(
        stored["provider_profiles"]["ark"]["models"],
        json!(["ark-image"])
    );
    assert_eq!(
        stored["provider_profiles"]["dashscope"]["models"],
        json!(["qwen-image-custom"])
    );
    assert_eq!(
        repository
            .model_api_key("multimodal", Some("ark"))
            .expect("Ark key")["api_key"],
        "ark-key"
    );
    fs::remove_dir_all(root).expect("remove test data");
}
