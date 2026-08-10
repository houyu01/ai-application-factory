//! Placeholder-composite model worker that renders layout references without changing user-selected shot references.

use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    value::{NOT_GENERATED, SUCCEEDED},
};

use super::DurableWorker;

impl DurableWorker {
    pub(super) fn placeholder_image(
        &self,
        id: &str,
        project_id: &str,
        task: &Value,
    ) -> AppResult<()> {
        let asset_id = task["input_snapshot"]["asset_id"]
            .as_str()
            .or_else(|| task["resource_id"].as_str())
            .unwrap_or_default();
        let project = self.repository.get_drama(project_id)?;
        let asset = self.repository.get_asset(project_id, asset_id)?;
        let metadata = &asset["metadata"];
        let ids = metadata["reference_asset_ids"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let assets = project["assets"].as_array().cloned().unwrap_or_default();
        let mut references = Vec::new();
        for reference in &ids {
            let reference_id = reference.as_str().unwrap_or_default();
            let item = assets
                .iter()
                .find(|item| item["id"].as_str() == Some(reference_id))
                .ok_or_else(|| {
                    AppError::BadRequest("占位图引用的场景、角色或道具图片不可用".to_owned())
                })?;
            let url = item["image_url"]
                .as_str()
                .and_then(|url| self.media.provider_reference_url(url))
                .ok_or_else(|| {
                    AppError::BadRequest("占位图引用的场景、角色或道具图片不可用".to_owned())
                })?;
            references.push(url);
        }
        let url = self.providers.image(
            asset["prompt"].as_str().unwrap_or_default(),
            project["ratio"].as_str().unwrap_or("9:16"),
            &references,
            project["multimodal_model"].as_str(),
        )?;
        self.repository
            .set_asset_image(project_id, asset_id, &url, "generated", SUCCEEDED)?;
        let shot_id = metadata["shot_id"].as_str().unwrap_or_default();
        if self.repository.get_shot(project_id, shot_id).is_ok() {
            self.repository.save_placeholder_layout(
                project_id,
                shot_id,
                Map::from_iter([
                    ("shot_id".to_owned(), json!(shot_id)),
                    (
                        "scene_asset_id".to_owned(),
                        metadata["scene_asset_id"].clone(),
                    ),
                    ("placements".to_owned(), metadata["placements"].clone()),
                ]),
            )?;
            self.repository
                .set_shot_status(project_id, shot_id, NOT_GENERATED)?;
        }
        self.repository.finish_drama_task(id, SUCCEEDED, Some(json!({"asset_id":asset_id,"image_url":url,"scene_asset_id":metadata["scene_asset_id"],"placements":metadata["placements"],"reference_asset_ids":ids,"render_mode":"generated_composite"})), None)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use serde_json::{json, Map};

    use crate::{
        db::Database,
        media::MediaStore,
        repository::Repository,
        value::{new_id, GENERATING, SUCCEEDED},
    };

    use super::DurableWorker;

    fn image_endpoint() -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind image provider");
        let endpoint = format!(
            "http://{}/images/generations",
            listener.local_addr().expect("address")
        );
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("image request");
            let mut request = [0_u8; 4_096];
            let _ = stream.read(&mut request).expect("read image request");
            let body = r#"{"data":[{"b64_json":"iVBORw0KGgo="}]}"#;
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).expect("write image response");
        });
        (endpoint, server)
    }

    #[test]
    fn completed_placeholder_does_not_change_manual_shot_references() {
        let root = std::env::temp_dir().join(format!("placeholder-references-{}", new_id()));
        let repository = Repository::new(Database::open(root.join("test.db")).expect("database"));
        let project = repository
            .create_drama(Map::from_iter([
                ("name".to_owned(), json!("占位图手动参考")),
                (
                    "script".to_owned(),
                    json!("林岩和苏晚在旧居外发现了重要线索，并决定继续调查。"),
                ),
            ]))
            .expect("project");
        let project_id = project["id"].as_str().expect("project id");
        let bootstrap_id = project["task_id"].as_str().expect("bootstrap id");
        repository.save_drama_decomposition(project_id, &json!({"episodes":[{"name":"第1集","shots":[{"title":"旧居外","original_text":"林岩和苏晚站在旧居外。","duration_seconds":10}]}],"assets":[]})).expect("decomposition");
        repository
            .finish_drama_task(bootstrap_id, SUCCEEDED, None, None)
            .expect("bootstrap complete");
        let shot_id = repository.get_drama(project_id).expect("detail")["shots"][0]["id"]
            .as_str()
            .expect("shot id")
            .to_owned();
        let media = MediaStore::new(repository.clone()).expect("media");
        let image = media
            .save_data_url("data:image/png;base64,iVBORw0KGgo=")
            .expect("reference image");
        let scene = repository
            .create_asset(
                project_id,
                Map::from_iter([
                    ("type".to_owned(), json!("scene")),
                    ("name".to_owned(), json!("旧居")),
                    ("prompt".to_owned(), json!("旧居夜景")),
                ]),
            )
            .expect("scene");
        let character = repository
            .create_asset(
                project_id,
                Map::from_iter([
                    ("type".to_owned(), json!("character")),
                    ("name".to_owned(), json!("林岩")),
                    ("prompt".to_owned(), json!("年轻男性")),
                ]),
            )
            .expect("character");
        for asset in [&scene, &character] {
            repository
                .mark_asset_succeeded(project_id, asset["id"].as_str().expect("asset id"), &image)
                .expect("ready reference");
        }
        let manual_reference = json!({"type":"reference","asset_id":scene["id"],"asset_type":"scene","label":"旧居","image_url":image});
        repository
            .update_shot(
                project_id,
                &shot_id,
                Map::from_iter([
                    ("prompt".to_owned(), json!("用户手动选择旧居作为参考图。")),
                    ("prompt_rich".to_owned(), json!([manual_reference])),
                    ("reference_asset_ids".to_owned(), json!([scene["id"]])),
                ]),
            )
            .expect("manual reference");
        let placeholder = repository.create_asset(project_id, Map::from_iter([("type".to_owned(), json!("placeholder")), ("name".to_owned(), json!("旧居外占位图")), ("prompt".to_owned(), json!("生成干净的构图参考图")), ("metadata".to_owned(), json!({"shot_id":shot_id,"scene_asset_id":scene["id"],"placements":[],"reference_asset_ids":[scene["id"],character["id"]],"render_mode":"generated_composite"}))])).expect("placeholder");
        let placeholder_id = placeholder["id"].as_str().expect("placeholder id");
        repository
            .set_asset_status(project_id, placeholder_id, GENERATING)
            .expect("placeholder generating");
        let task = repository
            .create_active_drama_task(
                project_id,
                "placeholder_image",
                Some(placeholder_id),
                json!({"asset_id":placeholder_id}),
            )
            .expect("placeholder task");
        let (endpoint, server) = image_endpoint();
        repository.set_setting("multimodal", json!({"provider":"ark","api_key":"test-key","endpoint":endpoint,"model":"mock-image","models":["mock-image"]})).expect("image config");
        let worker = DurableWorker::new(repository.clone(), media).expect("worker");
        assert!(worker.process_once().expect("placeholder processed"));
        server.join().expect("image provider complete");
        assert_eq!(
            repository
                .get_drama_task(task["id"].as_str().expect("task id"))
                .expect("task")["status"],
            SUCCEEDED
        );
        let shot = repository.get_shot(project_id, &shot_id).expect("shot");
        assert_eq!(shot["prompt"], "用户手动选择旧居作为参考图。");
        assert_eq!(shot["prompt_rich"], json!([manual_reference]));
        assert_eq!(shot["reference_asset_ids"], json!([scene["id"]]));
        fs::remove_dir_all(root).expect("remove test data");
    }
}
