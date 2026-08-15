//! ZIP-specific publication operations owned by the media storage boundary.

use std::{fs, path::Path};

use crate::{
    error::{AppError, AppResult},
    media::MediaStore,
    storage::StorageConfig,
    value::new_id,
};

impl MediaStore {
    /// Keep a finished ZIP in app-private local media until the desktop save dialog copies it out.
    pub fn save_local_video_export_zip(&self, source: &Path) -> AppResult<String> {
        let media_id = format!("{}.zip", new_id().replace('-', ""));
        fs::copy(source, self.root.join(&media_id))?;
        Ok(format!("/api/media/{media_id}"))
    }

    /// Publish a finished ZIP to the explicitly configured cloud object store.
    pub fn save_cloud_video_export_zip(&self, source: &Path) -> AppResult<String> {
        let config = StorageConfig::from_values(
            self.repository
                .setting("storage")?
                .as_object()
                .cloned()
                .unwrap_or_default(),
        )?;
        if config.provider == "local" {
            return Err(AppError::BadRequest(
                "请先在设置中配置云端媒体存储".to_owned(),
            ));
        }
        self.save(&fs::read(source)?, "zip", "application/zip")
    }

    /// Preserve publication behavior for queued exports created before explicit destinations existed.
    pub fn save_legacy_video_export_zip(&self, source: &Path) -> AppResult<String> {
        let provider = self.repository.setting("storage")?["provider"]
            .as_str()
            .unwrap_or("local")
            .to_owned();
        if provider == "local" {
            self.save_local_video_export_zip(source)
        } else {
            self.save_cloud_video_export_zip(source)
        }
    }

    /// Copy the task-owned ZIP into the creator-selected desktop destination.
    pub fn copy_video_export_zip(&self, source_url: &str, destination: &Path) -> AppResult<()> {
        self.copy_for_video_export(source_url, destination)
    }
}
