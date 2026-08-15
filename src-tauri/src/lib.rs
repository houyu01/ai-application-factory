//! Local-first Tauri shell: all project state lives in Rust-managed SQLite, never in a Python service.

mod api;
mod api_game_routes;
mod db;
mod desktop_export_save;
mod error;
mod mapping;
mod media;
mod media_protocol;
mod media_video_export;
mod migration;
mod planner;
mod providers;
mod repository;
mod service;
mod service_cancellation;
mod service_deletion;
mod service_video;
mod skills;
mod storage;
mod system_voice_samples;
mod value;
mod volcengine_tts;
mod worker;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_api_routes;
#[cfg(test)]
mod tests_asset_contracts;
#[cfg(test)]
mod tests_game_frame_references;
#[cfg(test)]
mod tests_game_generation_failures;
#[cfg(test)]
mod tests_game_graph_editing;
#[cfg(test)]
mod tests_game_material_images;
#[cfg(test)]
mod tests_game_regeneration;
#[cfg(test)]
mod tests_game_rich_prompts;
#[cfg(test)]
mod tests_game_state;
#[cfg(test)]
mod tests_game_video_tasks;
#[cfg(test)]
mod tests_game_workflows;
#[cfg(test)]
mod tests_project_contracts;
#[cfg(test)]
mod tests_prompt_references;
#[cfg(test)]
mod tests_provider_images;
#[cfg(test)]
mod tests_provider_profiles;
#[cfg(test)]
mod tests_task_queue_states;
#[cfg(test)]
mod tests_video_exports;
#[cfg(test)]
mod tests_video_refinement;
#[cfg(test)]
mod tests_video_storage;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::path::PathBuf;

use serde_json::json;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri::path::BaseDirectory;
use tauri::{Manager, State};

use crate::{
    api::{ApiRequest, ApiResponse},
    service::DesktopService,
};

/// Serve the existing frontend's HTTP-shaped request through Tauri IPC without blocking the desktop event loop.
#[tauri::command]
async fn api_request(
    state: State<'_, DesktopService>,
    request: ApiRequest,
) -> Result<ApiResponse, String> {
    let service = state.inner().clone();
    Ok(
        match tauri::async_runtime::spawn_blocking(move || api::handle(&service, request)).await {
            Ok(response) => response,
            Err(error) => ApiResponse {
                status: 500,
                body: json!({"detail": format!("桌面请求执行失败：{error}")}),
                content_type: "application/json; charset=utf-8".to_owned(),
            },
        },
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .register_uri_scheme_protocol("media", |context, request| {
            let service = context.app_handle().state::<DesktopService>();
            let media_id = request.uri().path().trim_start_matches('/');
            media_protocol::response(
                service.media.path_for(media_id),
                request
                    .headers()
                    .get("range")
                    .and_then(|value| value.to_str().ok()),
            )
        })
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            #[cfg(any(target_os = "android", target_os = "ios"))]
            let skill_initialization = skills::initialize_embedded();
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            let skill_directory = if cfg!(debug_assertions) {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/skills")
            } else {
                app.path()
                    .resolve("resources/skills", BaseDirectory::Resource)?
            };
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            let skill_initialization = skills::initialize(skill_directory);
            skill_initialization
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            let data_dir = app.path().app_data_dir()?;
            let service = DesktopService::open(data_dir)
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            app.manage(service);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api_request,
            desktop_export_save::save_video_export
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AI Application Factory");
}
