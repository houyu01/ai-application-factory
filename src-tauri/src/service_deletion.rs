//! Media-aware deletion flows that keep local SQLite removal separate from best-effort object cleanup.

use serde_json::{json, Value};

use crate::{error::AppResult, service::DesktopService};

impl DesktopService {
    /// Delete a full drama project, then reclaim only media URLs owned by its local aggregate.
    pub fn delete_project(&self, project_id: &str) -> AppResult<Value> {
        let project = self.repository.get_drama(project_id)?;
        let urls = collect_media_urls(&project);
        self.repository.delete_drama(project_id)?;
        let (media_deleted, errors) = self.delete_media(urls);
        let mut result = json!({"status":"deleted","id":project_id,"media_deleted":media_deleted});
        if !errors.is_empty() {
            result["media_cleanup_errors"] = json!(errors);
        }
        Ok(result)
    }

    /// Delete an editable shot while retaining the adjacent-shot selection expected by the storyboard editor.
    pub fn delete_project_shot(&self, project_id: &str, shot_id: &str) -> AppResult<Value> {
        let project = self.repository.get_drama(project_id)?;
        let shots = project["shots"].as_array().cloned().unwrap_or_default();
        let index = shots
            .iter()
            .position(|shot| shot["id"].as_str() == Some(shot_id))
            .ok_or_else(|| {
                crate::error::AppError::NotFound(format!("Shot not found: {shot_id}"))
            })?;
        let next = shots
            .iter()
            .skip(index + 1)
            .find_map(|shot| shot["id"].as_str())
            .or_else(|| {
                index
                    .checked_sub(1)
                    .and_then(|prior| shots.get(prior))
                    .and_then(|shot| shot["id"].as_str())
            });
        let active_videos =
            self.repository
                .active_drama_tasks(project_id, "shot_video", Some(shot_id))?;
        let urls = collect_media_urls(&shots[index]);
        let mut result = self.repository.delete_shot(project_id, shot_id)?;
        let (_, cleanup_errors) = self.delete_media(urls);
        let provider_errors = active_videos
            .iter()
            .filter_map(|task| task["provider_task_id"].as_str())
            .filter(|id| !id.is_empty())
            .filter_map(|id| {
                crate::providers::ProviderClient::new(self.repository.clone(), self.media.clone())
                    .and_then(|client| client.cancel_video(id))
                    .err()
                    .map(|error| error.to_string())
            })
            .collect::<Vec<_>>();
        result["next_shot_id"] = next.map_or(Value::Null, |id| json!(id));
        if !cleanup_errors.is_empty() {
            result["media_cleanup_errors"] = json!(cleanup_errors);
        }
        if !provider_errors.is_empty() {
            result["provider_cancel_errors"] = json!(provider_errors);
        }
        Ok(result)
    }

    /// Delete an interaction project graph before cleaning its owned local or object-store media.
    pub fn delete_game(&self, game_id: &str) -> AppResult<Value> {
        let game = self.repository.get_game(game_id)?;
        let urls = collect_media_urls(&game);
        self.repository.delete_game(game_id)?;
        let (media_deleted, errors) = self.delete_media(urls);
        let mut result = json!({"status":"deleted","id":game_id,"media_deleted":media_deleted});
        if !errors.is_empty() {
            result["media_cleanup_errors"] = json!(errors);
        }
        Ok(result)
    }

    fn delete_media(&self, urls: Vec<String>) -> (i64, Vec<String>) {
        let mut deleted = 0;
        let mut errors = Vec::new();
        for url in urls {
            match self.media.delete_url(Some(&url)) {
                Ok(true) => deleted += 1,
                Ok(false) => {}
                Err(error) => errors.push(error.to_string()),
            }
        }
        (deleted, errors)
    }
}

fn collect_media_urls(value: &Value) -> Vec<String> {
    fn visit(value: &Value, urls: &mut Vec<String>) {
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    if ["image_url", "video_url", "url"].contains(&key.as_str()) {
                        if let Some(url) = child.as_str().filter(|url| !url.trim().is_empty()) {
                            if !urls.contains(&url.to_owned()) {
                                urls.push(url.to_owned());
                            }
                        }
                    } else {
                        visit(child, urls);
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    visit(item, urls);
                }
            }
            _ => {}
        }
    }
    let mut urls = Vec::new();
    visit(value, &mut urls);
    urls
}
