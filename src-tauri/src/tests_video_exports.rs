//! Regression coverage for persisted ZIP-export video choices.

use std::fs;

use serde_json::{json, Map};

use crate::{
    db::Database,
    planner,
    repository::Repository,
    value::{new_id, SUCCEEDED},
};

fn repository() -> (Repository, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("ai-video-export-test-{}", new_id()));
    let database = Database::open(root.join("ai_application_factory.db")).expect("database");
    (Repository::new(database), root)
}

#[test]
fn selected_versions_are_persisted_and_frozen_in_export_snapshot() {
    let (repository, root) = repository();
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("导出版本短剧")),
            (
                "script".to_owned(),
                json!("林岩在旧宅发现一段录音，苏晚决定陪他一起寻找录音的来历。"),
            ),
        ]))
        .expect("project");
    let project_id = project["id"].as_str().expect("project id");
    let plan = planner::fallback_drama_plan(
        "林岩在旧宅发现一段录音，苏晚决定陪他一起寻找录音的来历。",
        "真人风格",
        "都市",
        30,
    );
    repository
        .save_drama_decomposition(project_id, &plan)
        .expect("decomposition");
    let shots = repository.get_drama(project_id).expect("detail")["shots"]
        .as_array()
        .expect("shots")
        .clone();
    let mut selections = Vec::new();
    for (index, shot) in shots.iter().enumerate() {
        let shot_id = shot["id"].as_str().expect("shot id");
        let first = repository
            .create_shot_version(project_id, shot_id, &format!("task-{index}-1"), "prompt")
            .expect("first version");
        let first_id = first["id"].as_str().expect("first id");
        repository
            .finish_shot_version(
                project_id,
                shot_id,
                first_id,
                SUCCEEDED,
                Some(&format!("/api/media/{index}-1.mp4")),
                None,
            )
            .expect("finish first");
        let selected_id = if index == 0 {
            let second = repository
                .create_shot_version(project_id, shot_id, &format!("task-{index}-2"), "prompt")
                .expect("second version");
            let second_id = second["id"].as_str().expect("second id");
            repository
                .finish_shot_version(
                    project_id,
                    shot_id,
                    second_id,
                    SUCCEEDED,
                    Some("/api/media/new.mp4"),
                    None,
                )
                .expect("finish second");
            repository
                .select_shot_version_for_export(project_id, shot_id, second_id)
                .expect("mark second");
            second_id.to_owned()
        } else {
            repository
                .select_shot_version_for_export(project_id, shot_id, first_id)
                .expect("mark first");
            first_id.to_owned()
        };
        selections.push(json!({"shot_id":shot_id,"version_id":selected_id}));
    }
    let snapshot = repository
        .video_export_snapshot(
            project_id,
            &Map::from_iter([("selections".to_owned(), json!(selections))]),
        )
        .expect("snapshot");
    assert_eq!(snapshot.as_array().expect("entries").len(), shots.len());
    assert!(snapshot
        .as_array()
        .expect("entries")
        .iter()
        .all(|item| item["video_url"].as_str().is_some()));
    let first_shot = shots[0]["id"].as_str().expect("first shot");
    let versions = repository
        .shot_versions(project_id, first_shot)
        .expect("versions");
    assert_eq!(
        versions
            .iter()
            .filter(|version| version["is_selected_for_export"] == 1)
            .count(),
        1
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn export_snapshot_skips_shots_without_a_selected_video_version() {
    let (repository, root) = repository();
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("部分导出短剧")),
            (
                "script".to_owned(),
                json!("林岩在旧宅发现一段录音，苏晚决定陪他一起寻找录音的来历。"),
            ),
        ]))
        .expect("project");
    let project_id = project["id"].as_str().expect("project id");
    let plan = planner::fallback_drama_plan(
        "林岩在旧宅发现一段录音，苏晚决定陪他一起寻找录音的来历。",
        "真人风格",
        "都市",
        2,
    );
    repository
        .save_drama_decomposition(project_id, &plan)
        .expect("decomposition");
    let detail = repository.get_drama(project_id).expect("detail");
    let shots = detail["shots"].as_array().expect("shots");
    let selected_shot = shots.first().expect("selected shot")["id"]
        .as_str()
        .expect("selected shot id");
    let missing_shot = shots.get(1).expect("missing shot")["id"]
        .as_str()
        .expect("missing shot id");
    let version = repository
        .create_shot_version(project_id, selected_shot, "task-partial", "prompt")
        .expect("version");
    let version_id = version["id"].as_str().expect("version id");
    repository
        .finish_shot_version(
            project_id,
            selected_shot,
            version_id,
            SUCCEEDED,
            Some("/api/media/partial.mp4"),
            None,
        )
        .expect("finish version");
    let snapshot = repository
        .video_export_snapshot(
            project_id,
            &Map::from_iter([(
                "selections".to_owned(),
                json!([
                    {"shot_id": selected_shot, "version_id": version_id},
                    {"shot_id": missing_shot, "version_id": ""},
                ]),
            )]),
        )
        .expect("partial snapshot");
    let entries = snapshot.as_array().expect("entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["shot_id"], selected_shot);
    assert_eq!(entries[0]["video_url"], "/api/media/partial.mp4");
    fs::remove_dir_all(root).expect("cleanup");
}
