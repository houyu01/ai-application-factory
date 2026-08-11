//! Game-manifest and video-deletion API helpers separated from the main local router.

use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    service::DesktopService,
    value::object,
};

/// Route all interactive-game workbench requests away from the shared drama router while preserving HTTP-shaped responses.
pub(crate) fn handle_game_route(
    service: &DesktopService,
    method: &str,
    parts: &[&str],
    raw_body: Option<&str>,
) -> AppResult<Option<(u16, Value)>> {
    let body = || game_body(raw_body);
    let response = match (method, parts) {
        ("GET", ["games"]) => (200, json!(service.repository.list_games()?)),
        ("POST", ["games"]) => (202, service.repository.create_game(body()?)?),
        ("GET", ["games", id]) => (200, service.repository.get_game(id)?),
        ("PUT", ["games", id]) => (200, service.repository.save_game_editor(id, body()?)?),
        ("DELETE", ["games", id]) => (200, service.delete_game(id)?),
        ("PUT", ["games", id, "models"]) => {
            (200, service.repository.update_game_models(id, body()?)?)
        }
        ("PUT", ["games", id, "parameters"]) => {
            (200, service.repository.update_game_parameters(id, body()?)?)
        }
        ("PUT", ["games", id, "expanded-script"]) => {
            (200, service.repository.update_game_screenplay(id, body()?)?)
        }
        ("POST", ["games", id, "expanded-script", "continue"]) => {
            (202, service.repository.continue_game_screenplay(id)?)
        }
        ("POST", ["games", id, "expanded-script", "cancel"]) => {
            (202, service.repository.cancel_game_screenplay(id)?)
        }
        ("POST", ["games", id, "script-decomposition", "retry"]) => {
            (202, service.repository.retry_game_generation(id)?)
        }
        ("POST", ["games", id, "covers", "generate"]) => {
            (202, service.enqueue_game_cover(id, body()?)?)
        }
        ("POST", ["games", id, "cover-references"]) => {
            (200, service.upload_game_cover_reference(id, body()?)?)
        }
        ("GET", ["games", id, "runtime-manifest"]) => return game_manifest(service, id).map(Some),
        ("POST", ["games", id, "sessions"]) => (201, service.repository.create_game_session(id)?),
        ("GET", ["games", id, "sessions", session]) => {
            (200, service.repository.get_game_session(id, session)?)
        }
        ("POST", ["games", id, "sessions", session, "choices"]) => {
            let values = body()?;
            let edge = values
                .get("edge_id")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::BadRequest("缺少 edge_id".to_owned()))?;
            (200, service.repository.choose_game_edge(id, session, edge)?)
        }
        ("PUT", ["games", id, "asset-public-prompt"]) => (
            200,
            service
                .repository
                .update_game_asset_public_prompt(id, body()?)?,
        ),
        ("POST", ["games", id, "assets"]) => {
            (200, service.repository.create_game_asset(id, body()?)?)
        }
        ("POST", ["games", id, "assets", "images", "batch"]) => {
            let kind = body()?
                .get("asset_type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            (
                202,
                json!({"tasks":service.repository.enqueue_game_asset_images(id, &kind)?}),
            )
        }
        ("POST", ["games", id, "assets", asset, "image"]) => {
            (202, service.repository.enqueue_game_asset_image(id, asset)?)
        }
        ("DELETE", ["games", id, "assets", asset]) => {
            (200, service.repository.delete_game_asset(id, asset)?)
        }
        ("PUT", ["games", id, "assets", asset]) => (
            200,
            service.repository.update_game_asset(id, asset, body()?)?,
        ),
        ("POST", ["games", id, "assets", asset, "variants"]) => (
            200,
            service
                .repository
                .create_game_asset_variant(id, asset, body()?)?,
        ),
        ("PUT", ["games", id, "assets", asset, "variants", variant]) => (
            200,
            service
                .repository
                .update_game_asset_variant(id, asset, variant, body()?)?,
        ),
        ("DELETE", ["games", id, "assets", asset, "variants", variant]) => (
            200,
            service
                .repository
                .delete_game_asset_variant(id, asset, variant)?,
        ),
        ("POST", ["games", id, "assets", asset, "variants", variant, "image"]) => (
            202,
            service
                .repository
                .enqueue_game_asset_variant_image(id, asset, variant)?,
        ),
        ("PUT", ["games", id, "nodes", node, "placeholder-layout"]) => (
            200,
            service.save_game_placeholder_layout(id, node, body()?)?,
        ),
        ("POST", ["games", id, "nodes", node, "placeholders", "image"]) => {
            (202, service.enqueue_game_placeholder(id, node, body()?)?)
        }
        ("POST", ["games", id, "nodes", node, "video"]) => {
            (202, service.enqueue_game_node_video(id, node)?)
        }
        ("POST", ["games", id, "nodes", node, "video", "cancel"]) => {
            (202, service.cancel_game_node_video(id, node)?)
        }
        // The node-history check control updates the durable editor and runtime default, then returns the refreshed node.
        ("PUT", ["games", id, "nodes", node, "videos", video, "use-selection"]) => (
            200,
            service.select_game_node_video_for_use(id, node, video)?,
        ),
        ("POST", ["games", id, "nodes", node, "videos", video, "refinement"]) => {
            let values = body()?;
            let prompt = values
                .get("refinement_prompt")
                .and_then(Value::as_str)
                .unwrap_or_default();
            (
                202,
                service.enqueue_game_node_video_refinement(id, node, video, prompt)?,
            )
        }
        ("DELETE", ["games", id, "nodes", node, "videos", video]) => {
            (200, service.delete_game_node_video(id, node, video)?)
        }
        ("PUT", ["games", id, "nodes", node]) => {
            (200, service.repository.update_game_node(id, node, body()?)?)
        }
        ("POST", ["games", id, "edges"]) => {
            (200, service.repository.create_game_edge(id, body()?)?)
        }
        ("PUT", ["games", id, "edges", edge]) => {
            (200, service.repository.update_game_edge(id, edge, body()?)?)
        }
        ("DELETE", ["games", id, "edges", edge]) => {
            service.repository.delete_game_edge(id, edge)?;
            (204, Value::Null)
        }
        ("GET", ["game-tasks", task]) => (200, service.repository.get_game_task(task)?),
        _ => return Ok(None),
    };
    Ok(Some(response))
}

fn game_body(raw_body: Option<&str>) -> AppResult<Map<String, Value>> {
    raw_body
        .map(|raw| serde_json::from_str(raw).map_err(AppError::from))
        .transpose()
        .and_then(|value| object(value.unwrap_or_else(|| json!({}))))
}

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
