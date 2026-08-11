//! Tauri IPC for saving a completed ZIP export outside the app-private media directory.

use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use crate::service::DesktopService;

/// Confirms the absolute location selected in the operating-system save dialog.
#[derive(Serialize)]
pub struct SavedVideoExport {
    /// Filesystem path chosen by the creator and written by the desktop service.
    pub saved_path: String,
}

/// Copy one completed ZIP export to the path the short-drama export dialog selected.
///
/// The frontend invokes this only after the native “另存为” dialog returns a user-chosen path.
/// It accepts a project and task identifier rather than a media URL so a webview can save only
/// the ZIP produced by its own completed export task, never an arbitrary local or remote file.
#[tauri::command]
pub async fn save_video_export(
    state: State<'_, DesktopService>,
    project_id: String,
    task_id: String,
    destination: String,
) -> Result<SavedVideoExport, String> {
    let destination = PathBuf::from(destination);
    let service = state.inner().clone();
    let saved_path = tauri::async_runtime::spawn_blocking(move || {
        service.save_video_export(&project_id, &task_id, &destination)?;
        Ok::<_, crate::error::AppError>(destination.display().to_string())
    })
    .await
    .map_err(|error| format!("保存视频 ZIP 任务中断：{error}"))?
    .map_err(|error| error.to_string())?;
    Ok(SavedVideoExport { saved_path })
}
