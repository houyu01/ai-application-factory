//! Regression coverage for Ark image endpoint compatibility and saved provider cards.

use std::fs;

use serde_json::{json, Map};

use crate::{
    db::Database,
    providers::{ark_image_probe_is_reachable, image_generation_endpoint, MODEL_PROBE_TIMEOUT},
    repository::Repository,
    value::new_id,
};

#[test]
fn settings_model_probes_have_a_short_bounded_timeout() {
    assert_eq!(MODEL_PROBE_TIMEOUT.as_secs(), 20);
}

#[test]
fn image_generation_endpoint_preserves_the_configured_url() {
    assert_eq!(
        image_generation_endpoint(
            "https://ark.cn-beijing.volces.com/api/plan/v3/images/generations"
        ),
        "https://ark.cn-beijing.volces.com/api/plan/v3/images/generations"
    );
    assert_eq!(
        image_generation_endpoint("https://ark.cn-beijing.volces.com/api/plan/v3"),
        "https://ark.cn-beijing.volces.com/api/plan/v3/images/generations"
    );
}

#[test]
fn saving_ark_image_settings_keeps_the_configured_url() {
    let root = std::env::temp_dir().join(format!("ark-image-settings-{}", new_id()));
    let repository = Repository::new(Database::open(root.join("settings.db")).expect("database"));
    let config = repository
        .model_config_candidate(&Map::from_iter([
            ("kind".to_owned(), json!("multimodal")),
            ("provider".to_owned(), json!("ark")),
            (
                "endpoint".to_owned(),
                json!("https://ark.cn-beijing.volces.com/api/plan/v3"),
            ),
            ("api_key".to_owned(), json!("test-key")),
        ]))
        .expect("candidate");
    assert_eq!(
        config["endpoint"],
        "https://ark.cn-beijing.volces.com/api/plan/v3"
    );
    fs::remove_dir_all(root).expect("remove test data");
}

#[test]
fn ark_image_probe_accepts_reachable_model_validation_responses() {
    assert!(ark_image_probe_is_reachable(
        reqwest::StatusCode::BAD_REQUEST
    ));
    assert!(ark_image_probe_is_reachable(
        reqwest::StatusCode::TOO_MANY_REQUESTS
    ));
    assert!(!ark_image_probe_is_reachable(
        reqwest::StatusCode::UNAUTHORIZED
    ));
    assert!(!ark_image_probe_is_reachable(
        reqwest::StatusCode::NOT_FOUND
    ));
}
