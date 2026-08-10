//! Regression coverage for version-scoped video-refinement persistence.

use std::fs;

use rusqlite::params;
use serde_json::{json, Map};

use crate::{
    db::Database,
    planner,
    repository::{Repository, ShotVersionInput, ShotVideoRefinement},
    value::{new_id, SUCCEEDED},
};

#[test]
fn refinement_feedback_stays_on_the_selected_video_and_new_version_keeps_its_source() {
    let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
    let database = Database::open(root.join("ai_application_factory.db")).expect("test database");
    let repository = Repository::new(database);
    let project = repository
        .create_drama(Map::from_iter([
            ("name".to_owned(), json!("视频微调短剧")),
            (
                "script".to_owned(),
                json!("林岩在雨夜赶往旧宅，想阻止苏晚打开那封尘封多年的信。"),
            ),
        ]))
        .expect("create project");
    let project_id = project["id"].as_str().expect("project id");
    let plan = planner::fallback_drama_plan(
        "林岩在雨夜赶往旧宅，想阻止苏晚打开那封尘封多年的信。",
        "真人风格",
        "都市",
        80,
    );
    repository
        .save_drama_decomposition(project_id, &plan)
        .expect("save plan");
    let shot = repository.get_drama(project_id).expect("project detail")["shots"][0].clone();
    let shot_id = shot["id"].as_str().expect("shot id");
    let source = repository.create_shot_version_with_input(project_id, shot_id, "source-task", ShotVersionInput {
        prompt: "雨夜中镜头缓慢推进旧宅大门".to_owned(),
        prompt_rich: json!([{"type":"reference","asset_id":"scene-1","snapshot_image_url":"/api/media/original.png"}]),
        structured: json!({"camera":"推进"}),
        refinement: None,
    }).expect("create source version");
    let source_id = source["id"].as_str().expect("source version id");
    repository
        .finish_shot_version(
            project_id,
            shot_id,
            source_id,
            SUCCEEDED,
            Some("/api/media/original.mp4"),
            None,
        )
        .expect("finish source");
    repository
        .set_shot_version_refinement_prompt(
            project_id,
            shot_id,
            source_id,
            "灯光更温暖，人物表情更克制",
        )
        .expect("save feedback");

    let refined = repository.create_shot_version_with_input(project_id, shot_id, "refined-task", ShotVersionInput {
        prompt: "雨夜中镜头缓慢推进旧宅大门".to_owned(),
        prompt_rich: json!([{"type":"reference","asset_id":"scene-1","snapshot_image_url":"/api/media/original.png"}]),
        structured: json!({"camera":"推进"}),
        refinement: Some(ShotVideoRefinement { source_version_id: source_id.to_owned(), source_video_url: "/api/media/original.mp4".to_owned() }),
    }).expect("create refined version");
    let source = repository
        .get_shot_version(project_id, shot_id, source_id)
        .expect("source version");
    let child = repository
        .get_shot_version(
            project_id,
            shot_id,
            refined["id"].as_str().expect("refined id"),
        )
        .expect("refined version");

    assert_eq!(source["refinement_prompt"], "灯光更温暖，人物表情更克制");
    let editor = repository
        .get_editor_drama(project_id, Some(shot_id))
        .expect("editor detail");
    assert_eq!(
        editor["shots"][0]["versions"]
            .as_array()
            .expect("editor versions")
            .iter()
            .find(|version| version["id"] == source_id)
            .expect("source editor version")["refinement_prompt"],
        "灯光更温暖，人物表情更克制"
    );
    assert_eq!(child["refinement_source_version_id"], source_id);
    assert_eq!(child["source_video_url"], "/api/media/original.mp4");
    assert_eq!(child["refinement_prompt"], "");
    assert_eq!(
        child["prompt_rich"][0]["snapshot_image_url"],
        "/api/media/original.png"
    );
    let child_id = refined["id"].as_str().expect("refined id").to_owned();
    drop(repository);
    let connection = rusqlite::Connection::open(root.join("ai_application_factory.db"))
        .expect("open copied legacy data");
    connection
        .execute(
            "UPDATE drama_shot_versions SET refinement_prompt=?1 WHERE id=?2",
            params!["灯光更温暖，人物表情更克制", child_id],
        )
        .expect("restore copied feedback");
    connection
        .execute(
            "DELETE FROM desktop_schema_migrations WHERE id='refinement_prompt_ownership_v1'",
            [],
        )
        .expect("reset compatibility migration");
    drop(connection);
    let migrated = Repository::new(
        Database::open(root.join("ai_application_factory.db")).expect("migrate copied feedback"),
    );
    assert_eq!(
        migrated
            .get_shot_version(project_id, shot_id, &child_id)
            .expect("migrated child")["refinement_prompt"],
        ""
    );
    fs::remove_dir_all(root).expect("remove test data");
}
