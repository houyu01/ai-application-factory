//! Game-manifest and video-deletion API helpers separated from the main local router.

use serde_json::{json, Value};

use crate::{error::AppResult, service::DesktopService};

/// Delete a selected shot-video version, local media, and its cancellable upstream provider task.
pub(crate) fn delete_video(
    service: &DesktopService,
    project: &str,
    shot: &str,
    video: &str,
) -> AppResult<Value> {
    let details = service.repository.get_shot(project, shot)?;
    let version = service
        .repository
        .shot_versions(project, shot)?
        .into_iter()
        .find(|item| item["id"].as_str() == Some(video));
    let url = details["historical_videos"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["id"].as_str() == Some(video)))
        .and_then(|item| item["url"].as_str());
    let mut result = service.repository.delete_shot_video(project, shot, video)?;
    let deleted = service.media.delete_url(url)?;
    result["media_deleted"] = json!(if deleted { 1 } else { 0 });
    if let Some(provider_id) = version
        .as_ref()
        .and_then(|item| item["provider_task_id"].as_str())
        .filter(|id| !id.is_empty())
    {
        if let Err(error) = crate::providers::ProviderClient::new(
            service.repository.clone(),
            service.media.clone(),
        )?
        .cancel_video(provider_id)
        {
            result["provider_cancel_errors"] = json!([error.to_string()]);
        }
    }
    Ok(result)
}

/// Build the game runtime manifest returned when the interactive game player opens.
pub(crate) fn game_manifest(service: &DesktopService, id: &str) -> AppResult<(u16, Value)> {
    let game = service.repository.get_game(id)?;
    let start = game["nodes"]
        .as_array()
        .and_then(|nodes| {
            nodes
                .iter()
                .find(|node| node["node_type"].as_str() == Some("start"))
        })
        .and_then(|node| node["id"].as_str());
    Ok((
        200,
        json!({"game_id":game["id"],"name":game["name"],"platform":game["platform"],"engine":if game["platform"].as_str()==Some("Steam游戏"){ "Unity" }else{"Cocos Creator"},"start_node_id":start,"nodes":game["nodes"],"edges":game["edges"]}),
    ))
}
