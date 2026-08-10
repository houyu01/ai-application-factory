//! HTTP-shaped in-process router preserving every frontend route without a localhost server.
use crate::{
    error::{AppError, AppResult},
    service::DesktopService,
    value::{object, GENERATING},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use url::Url;
/// Serialized request sent by the frontend fetch bridge through `tauri::invoke`.
#[derive(Debug, Deserialize)]
pub struct ApiRequest {
    pub method: String,
    pub path: String,
    pub body: Option<String>,
}
/// Serialized response used to construct a browser-standard `Response` in the fetch bridge.
#[derive(Debug, Serialize)]
pub struct ApiResponse {
    pub status: u16,
    pub body: Value,
    pub content_type: String,
}
/// Route a browser-equivalent API request to the Rust local service boundary.
pub fn handle(service: &DesktopService, request: ApiRequest) -> ApiResponse {
    match dispatch(service, &request) {
        Ok((status, body)) => ApiResponse {
            status,
            body,
            content_type: "application/json; charset=utf-8".to_owned(),
        },
        Err(error) => ApiResponse {
            status: error.status_code(),
            body: json!({"detail":error.to_string()}),
            content_type: "application/json; charset=utf-8".to_owned(),
        },
    }
}

fn dispatch(service: &DesktopService, request: &ApiRequest) -> AppResult<(u16, Value)> {
    let url = Url::parse(&format!(
        "http://desktop.local{}",
        if request.path.starts_with('/') {
            request.path.clone()
        } else {
            format!("/{}", request.path)
        }
    ))
    .map_err(|_| AppError::BadRequest("无效 API 路径".to_owned()))?;
    let decoded_parts = path_parts(&url)?;
    let parts = decoded_parts.iter().map(String::as_str).collect::<Vec<_>>();
    let body = || {
        request
            .body
            .as_deref()
            .map(|value| serde_json::from_str(value).map_err(AppError::from))
            .transpose()
            .and_then(|value| object(value.unwrap_or_else(|| json!({}))))
    };
    let method = request.method.to_uppercase();
    let task_status = if url.query_pairs().any(|(key, _)| key == "status") {
        query(&url, "status")
    } else {
        Some(GENERATING.to_owned())
    };
    let task_since = query(&url, "since");
    let selected_shot = query(&url, "shot_id");
    let template_scope = query(&url, "scope");
    let template_name = query(&url, "name");
    let include_inactive = query(&url, "include_inactive")
        .as_deref()
        .map(|value| value != "false")
        .unwrap_or(true);
    match (method.as_str(), parts.as_slice()) {
        ("GET", ["projects"]) => Ok((200, json!(service.repository.list_dramas()?))),
        ("POST", ["projects"]) => Ok((202, service.create_drama(body()?)?)),
        ("GET", ["projects", id]) => Ok((
            200,
            service
                .repository
                .get_editor_drama(id, selected_shot.as_deref())?,
        )),
        ("DELETE", ["projects", id]) => Ok((200, service.delete_project(id)?)),
        ("PUT", ["projects", id, "name"]) => {
            Ok((200, service.repository.update_drama(id, body()?)?))
        }
        ("PUT", ["projects", id, "models"]) => {
            Ok((200, service.repository.update_drama(id, body()?)?))
        }
        ("PUT", ["projects", id, "parameters"]) => {
            Ok((200, service.repository.update_drama(id, body()?)?))
        }
        ("PUT", ["projects", id, "video-public-prompt"]) => {
            Ok((200, service.repository.update_drama(id, body()?)?))
        }
        ("PUT", ["projects", id, "asset-public-prompt"]) => {
            update_asset_public_prompt(service, id, body()?)
        }
        ("GET", ["projects", id, "tasks"]) => Ok((
            200,
            service.repository.poll_drama_tasks(
                id,
                task_status.as_deref(),
                task_since.as_deref(),
            )?,
        )),
        ("GET", ["projects", id, "assets"]) => {
            Ok((200, json!(service.repository.list_assets(id)?)))
        }
        ("POST", ["projects", id, "assets"]) => {
            Ok((200, service.repository.create_asset(id, body()?)?))
        }
        ("DELETE", ["projects", id, "assets", asset]) => {
            Ok((200, service.repository.delete_asset(id, asset)?))
        }
        ("PUT", ["projects", id, "assets", asset]) => {
            Ok((200, service.repository.update_asset(id, asset, body()?)?))
        }
        ("POST", ["projects", id, "assets", asset, "upload"]) => {
            Ok((200, service.upload_asset(id, asset, body()?)?))
        }
        ("POST", ["projects", id, "assets", asset, "image"]) => {
            Ok((202, service.enqueue_asset_image(id, asset)?))
        }
        ("POST", ["projects", id, "assets", asset, "variants"]) => Ok((
            200,
            service
                .repository
                .create_asset_variant(id, asset, body()?)?,
        )),
        ("PUT", ["projects", id, "assets", asset, "variants", variant]) => Ok((
            200,
            service
                .repository
                .update_asset_variant(id, asset, variant, body()?)?,
        )),
        ("DELETE", ["projects", id, "assets", asset, "variants", variant]) => Ok((
            200,
            service
                .repository
                .delete_asset_variant(id, asset, variant)?,
        )),
        ("POST", ["projects", id, "assets", asset, "variants", variant, "image"]) => {
            Ok((202, service.enqueue_variant_image(id, asset, variant)?))
        }
        ("POST", ["projects", id, "assets", "images", "batch"]) => {
            Ok((202, service.enqueue_asset_batch(id, body()?)?))
        }
        ("POST", ["projects", id, "assets", kind, "images", "cancel"]) => {
            Ok((202, service.cancel_asset_images(id, kind)?))
        }
        ("GET", ["projects", id, "shots"]) => {
            let (shots, episodes) = service.repository.list_shots(id)?;
            Ok((200, json!({"shots":shots,"episodes":episodes})))
        }
        ("POST", ["projects", id, "shots"]) => {
            Ok((200, service.repository.create_shot(id, body()?)?))
        }
        ("PUT", ["projects", id, "shots", shot]) => {
            Ok((200, service.repository.update_shot(id, shot, body()?)?))
        }
        ("DELETE", ["projects", id, "shots", shot]) => {
            Ok((200, service.delete_project_shot(id, shot)?))
        }
        ("POST", ["projects", id, "shots", shot, "prompt"]) => {
            Ok((202, service.enqueue_shot_prompt(id, shot)?))
        }
        ("POST", ["projects", id, "shots", shot, "auto-match-references"]) => {
            Ok((202, service.enqueue_shot_prompt(id, shot)?))
        }
        ("POST", ["projects", id, "shots", shot, "quality"]) => {
            Ok((202, service.enqueue_shot_quality(id, shot)?))
        }
        ("POST", ["projects", id, "shots", shot, "video"]) => Ok((
            202,
            service
                .enqueue_shot_videos(id, shot, 1)?
                .into_iter()
                .next()
                .ok_or_else(|| AppError::External("没有创建视频任务".to_owned()))?,
        )),
        ("POST", ["projects", id, "shots", shot, "videos"]) => {
            let count = body()?.get("count").and_then(Value::as_i64).unwrap_or(1);
            let tasks = service.enqueue_shot_videos(id, shot, count)?;
            Ok((202, json!({"requested_count":count,"tasks":tasks})))
        }
        ("POST", ["projects", id, "videos", "serial"]) => {
            Ok((202, service.start_serial_shot_video_batch(id)?))
        }
        ("POST", ["projects", id, "videos", "serial", batch, "advance"]) => {
            let values = body()?;
            let last_frame = values.get("last_frame_data_url").and_then(Value::as_str);
            Ok((
                202,
                service.advance_serial_shot_video_batch(id, batch, last_frame)?,
            ))
        }
        ("POST", ["projects", id, "shots", shot, "videos", version, "refinement"]) => {
            let values = body()?;
            let prompt = values["refinement_prompt"].as_str().unwrap_or_default();
            Ok((
                202,
                service.enqueue_shot_video_refinement(id, shot, version, prompt)?,
            ))
        }
        ("GET", ["projects", id, "shots", shot, "versions"]) => {
            Ok((200, json!(service.repository.shot_versions(id, shot)?)))
        }
        ("PUT", ["projects", id, "shots", shot, "placeholder-layout"]) => Ok((
            200,
            service
                .repository
                .save_placeholder_layout(id, shot, body()?)?,
        )),
        ("POST", ["projects", id, "placeholders", "image"]) => {
            let values = body()?;
            let shot = values
                .get("shot_id")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::BadRequest("缺少 shot_id".to_owned()))?
                .to_owned();
            Ok((202, service.enqueue_placeholder(id, &shot, values)?))
        }
        ("POST", ["projects", id, "shots", shot, "reference-images", "generate"]) => {
            Ok((202, service.enqueue_reference_images(id, shot)?))
        }
        ("DELETE", ["projects", id, "shots", shot, "videos", video]) => Ok((
            200,
            crate::api_game_routes::delete_video(service, id, shot, video)?,
        )),
        ("POST", ["projects", id, "shots", shot, "video", "cancel"]) => {
            Ok((202, service.cancel_videos(id, Some(shot))?))
        }
        ("POST", ["projects", id, "videos", "cancel"]) => {
            Ok((202, service.cancel_videos(id, None)?))
        }
        ("GET", ["projects", id, "expanded-script"]) => Ok((200, service.expanded_screenplay(id)?)),
        ("PUT", ["projects", id, "expanded-script"]) => {
            Ok((200, service.repository.update_screenplay(id, body()?)?))
        }
        ("POST", ["projects", id, "expanded-script", "continue"]) => {
            Ok((202, service.continue_screenplay(id)?))
        }
        ("POST", ["projects", id, "expanded-script", "cancel"]) => {
            Ok((202, service.cancel_screenplay(id)?))
        }
        ("POST", ["projects", id, "script-decomposition", "retry"]) => {
            Ok((202, service.retry_decomposition(id)?))
        }
        ("POST", ["projects", id, "script-decomposition", "restart"]) => {
            Ok((202, service.restart_decomposition(id)?))
        }
        ("POST", ["projects", id, "script-decomposition", "regenerate"]) => {
            let values = body()?;
            Ok((
                202,
                service
                    .regenerate_decomposition(id, values.get("script").and_then(Value::as_str))?,
            ))
        }
        ("POST", ["projects", id, "covers", "generate"]) => {
            Ok((202, service.enqueue_cover(id, body()?)?))
        }
        ("GET", ["prompt-templates"]) => Ok((
            200,
            json!(service.repository.prompt_templates(
                template_scope.as_deref().unwrap_or("drama"),
                template_name.as_deref(),
                include_inactive
            )?),
        )),
        ("POST", ["prompt-templates"]) => {
            Ok((200, service.repository.create_prompt_template(body()?)?))
        }
        ("GET", ["settings", "models"]) => Ok((200, service.repository.model_configs()?)),
        ("PUT", ["settings", "models"]) => Ok((200, service.save_model_config(body()?)?)),
        ("PUT", ["settings", "models", kind, "options"]) => {
            Ok((200, service.repository.save_model_options(kind, body()?)?))
        }
        ("GET", ["settings", "models", kind, "api-key"]) => Ok((
            200,
            service
                .repository
                .model_api_key(kind, query(&url, "provider").as_deref())?,
        )),
        ("GET", ["settings", "voices"]) => Ok((200, json!(service.repository.voices()?))),
        // The settings catalog lets creators append reusable names and descriptions before assigning a character.
        ("POST", ["settings", "voices"]) => {
            Ok((201, service.repository.create_voice_preset(body()?)?))
        }
        ("GET", ["settings", "storage"]) => Ok((200, service.repository.storage_config()?)),
        ("PUT", ["settings", "storage"]) => Ok((200, service.save_storage(body()?)?)),
        ("GET", ["games"]) => Ok((200, json!(service.repository.list_games()?))),
        ("POST", ["games"]) => Ok((202, service.repository.create_game(body()?)?)),
        ("GET", ["games", id]) => Ok((200, service.repository.get_game(id)?)),
        ("DELETE", ["games", id]) => Ok((200, service.delete_game(id)?)),
        ("PUT", ["games", id, "models"]) => {
            Ok((200, service.repository.update_game_models(id, body()?)?))
        }
        ("GET", ["games", id, "runtime-manifest"]) => {
            crate::api_game_routes::game_manifest(service, id)
        }
        ("POST", ["games", id, "sessions"]) => {
            Ok((201, service.repository.create_game_session(id)?))
        }
        ("GET", ["games", id, "sessions", session]) => {
            Ok((200, service.repository.get_game_session(id, session)?))
        }
        ("POST", ["games", id, "sessions", session, "choices"]) => {
            let values = body()?;
            let edge = values
                .get("edge_id")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::BadRequest("缺少 edge_id".to_owned()))?
                .to_owned();
            Ok((
                200,
                service.repository.choose_game_edge(id, session, &edge)?,
            ))
        }
        ("POST", ["games", id, "nodes", node, "video"]) => {
            Ok((202, service.repository.enqueue_game_node_video(id, node)?))
        }
        ("PUT", ["games", id, "nodes", node]) => {
            Ok((200, service.repository.update_game_node(id, node, body()?)?))
        }
        ("POST", ["games", id, "edges"]) => {
            Ok((200, service.repository.create_game_edge(id, body()?)?))
        }
        ("PUT", ["games", id, "edges", edge]) => {
            Ok((200, service.repository.update_game_edge(id, edge, body()?)?))
        }
        ("DELETE", ["games", id, "edges", edge]) => {
            service.repository.delete_game_edge(id, edge)?;
            Ok((204, Value::Null))
        }
        ("GET", ["game-tasks", task]) => Ok((200, service.repository.get_game_task(task)?)),
        _ => Err(AppError::NotFound(format!(
            "未找到本地 API：{} {}",
            request.method,
            url.path()
        ))),
    }
}
fn query(url: &Url, name: &str) -> Option<String> {
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

/// Split a browser path into decoded route parameters without allowing encoded slashes to alter routing.
pub(crate) fn path_parts(url: &Url) -> AppResult<Vec<String>> {
    url.path_segments()
        .map(|items| {
            items
                .filter(|item| !item.is_empty())
                .map(decode_path_segment)
                .collect()
        })
        .unwrap_or_else(|| Ok(Vec::new()))
}

fn decode_path_segment(value: &str) -> AppResult<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        let encoded = bytes
            .get(index + 1..index + 3)
            .ok_or_else(|| AppError::BadRequest("API 路径包含无效的百分号编码".to_owned()))?;
        let hex = std::str::from_utf8(encoded)
            .ok()
            .and_then(|value| u8::from_str_radix(value, 16).ok())
            .ok_or_else(|| AppError::BadRequest("API 路径包含无效的百分号编码".to_owned()))?;
        decoded.push(hex);
        index += 3;
    }
    String::from_utf8(decoded)
        .map_err(|_| AppError::BadRequest("API 路径不是有效的 UTF-8".to_owned()))
}

fn update_asset_public_prompt(
    service: &DesktopService,
    id: &str,
    values: Map<String, Value>,
) -> AppResult<(u16, Value)> {
    let kind = values
        .get("asset_type")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::BadRequest("缺少 asset_type".to_owned()))?;
    if !["character", "scene", "prop"].contains(&kind) {
        return Err(AppError::BadRequest("不支持的素材类型".to_owned()));
    }
    let project = service.repository.get_drama(id)?;
    let mut prompts = project["asset_public_prompts"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    prompts.insert(
        kind.to_owned(),
        values
            .get("public_prompt")
            .cloned()
            .unwrap_or_else(|| json!("")),
    );
    Ok((
        200,
        service.repository.update_drama(
            id,
            Map::from_iter([("asset_public_prompts".to_owned(), Value::Object(prompts))]),
        )?,
    ))
}
