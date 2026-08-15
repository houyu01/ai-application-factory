//! Local FFmpeg assembly and ZIP packaging for a creator-selected set of short-drama video versions.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    value::{new_id, GENERATING, SUCCEEDED},
};

use super::{video_export_zip::write_zip, DurableWorker};

struct ExportSource {
    episode_id: String,
    episode_name: String,
    episode_order: i64,
    video_url: String,
}

/// Deletes the app-private staging directory after a completed, failed, or cancelled export attempt.
struct ScopedDirectory(PathBuf);

impl Drop for ScopedDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl DurableWorker {
    /// Assemble selected shot versions into one MP4 file per episode, then persist the requested ZIP for download.
    ///
    /// The short-drama export dialog has already frozen the version URLs in the durable task snapshot. This
    /// worker owns only temporary media transforms and archive output; the MediaStore owns final publication.
    pub(super) fn export_drama_videos(
        &self,
        task_id: &str,
        project_id: &str,
        task: &Value,
    ) -> AppResult<()> {
        if task["input_snapshot"]["format"].as_str() != Some("mp4") {
            return Err(AppError::BadRequest("视频导出格式仅支持 mp4".to_owned()));
        }
        let sources = export_sources(&task["input_snapshot"])?;
        let staging = std::env::temp_dir().join(format!("ai-video-export-{}", new_id()));
        fs::create_dir_all(&staging)?;
        let staging = ScopedDirectory(staging);
        self.repository
            .update_drama_task_progress(task_id, 3, "正在整理导出视频")?;
        let local_sources =
            self.materialize_export_sources(task_id, project_id, &sources, &staging.0)?;
        let episodes = group_by_episode(sources, local_sources);
        let episode_count = episodes.len().max(1) as i64;
        let mut outputs = Vec::with_capacity(episodes.len());
        for (index, ((episode_order, episode_name, _episode_id), paths)) in
            episodes.into_iter().enumerate()
        {
            self.ensure_export_active(project_id, task_id)?;
            let base = 26 + (index as i64 * 58 / episode_count);
            self.repository.update_drama_task_progress(
                task_id,
                base,
                &format!("正在拼接{episode_name}"),
            )?;
            let assembled = staging.0.join(format!("episode-{index:03}.mp4"));
            self.concat_episode(task_id, project_id, &paths, &assembled)?;
            outputs.push((
                format!(
                    "{:03}_{}.mp4",
                    episode_order.max(1),
                    safe_name(&episode_name)
                ),
                assembled,
            ));
        }
        self.ensure_export_active(project_id, task_id)?;
        self.repository
            .update_drama_task_progress(task_id, 88, "正在打包 ZIP")?;
        let archive = staging.0.join("short-drama-export.zip");
        write_zip(&archive, &outputs)?;
        self.ensure_export_active(project_id, task_id)?;
        self.repository
            .update_drama_task_progress(task_id, 96, "正在保存下载文件")?;
        let destination = task["input_snapshot"]["destination"].as_str();
        let url = match destination {
            Some("local") => self.media.save_local_video_export_zip(&archive)?,
            Some("cloud") => self.media.save_cloud_video_export_zip(&archive)?,
            _ => self.media.save_legacy_video_export_zip(&archive)?,
        };
        let project = self.repository.get_drama(project_id)?;
        let file_name = format!(
            "{}-视频合集.zip",
            safe_name(project["name"].as_str().unwrap_or("短剧"))
        );
        self.repository.finish_drama_task(
            task_id,
            SUCCEEDED,
            Some(json!({"url":url,"file_name":file_name,"format":"mp4","destination":destination.unwrap_or("legacy"),"episode_count":outputs.len()})),
            None,
        )?;
        Ok(())
    }

    fn materialize_export_sources(
        &self,
        task_id: &str,
        project_id: &str,
        sources: &[ExportSource],
        directory: &Path,
    ) -> AppResult<Vec<PathBuf>> {
        sources
            .iter()
            .enumerate()
            .map(|(index, source)| {
                self.ensure_export_active(project_id, task_id)?;
                self.repository.update_drama_task_progress(
                    task_id,
                    4 + (index as i64 * 20 / sources.len().max(1) as i64),
                    &format!("正在读取{}的视频", source.episode_name),
                )?;
                let destination = directory.join(format!("source-{index:04}.mp4"));
                self.media
                    .copy_for_video_export(&source.video_url, &destination)?;
                Ok(destination)
            })
            .collect()
    }

    fn concat_episode(
        &self,
        task_id: &str,
        project_id: &str,
        sources: &[PathBuf],
        output: &Path,
    ) -> AppResult<()> {
        let list = output.with_extension("txt");
        let entries = sources
            .iter()
            .map(|path| format!("file '{}'\n", escape_concat_path(path)))
            .collect::<String>();
        fs::write(&list, entries)?;
        let copy_args = vec![
            "-y".to_owned(),
            "-f".to_owned(),
            "concat".to_owned(),
            "-safe".to_owned(),
            "0".to_owned(),
            "-i".to_owned(),
            list.display().to_string(),
            "-c".to_owned(),
            "copy".to_owned(),
            output.display().to_string(),
        ];
        if self.run_ffmpeg(task_id, project_id, &copy_args).is_ok() {
            return Ok(());
        }
        self.ensure_export_active(project_id, task_id)?;
        let reencode_args = vec![
            "-y".to_owned(),
            "-f".to_owned(),
            "concat".to_owned(),
            "-safe".to_owned(),
            "0".to_owned(),
            "-i".to_owned(),
            list.display().to_string(),
            "-c:v".to_owned(),
            "libx264".to_owned(),
            "-c:a".to_owned(),
            "aac".to_owned(),
            output.display().to_string(),
        ];
        self.run_ffmpeg(task_id, project_id, &reencode_args)
    }

    fn run_ffmpeg(&self, task_id: &str, project_id: &str, args: &[String]) -> AppResult<()> {
        let mut child = Command::new(ffmpeg_program())
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| AppError::External(format!("无法启动 FFmpeg：{error}")))?;
        loop {
            if let Some(status) = child.try_wait()? {
                return status.success().then_some(()).ok_or_else(|| {
                    AppError::External(
                        "FFmpeg 未能拼接所选视频，请确认视频文件完整且编码兼容".to_owned(),
                    )
                });
            }
            if self.ensure_export_active(project_id, task_id).is_err() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::BadRequest("视频打包已取消".to_owned()));
            }
            thread::sleep(Duration::from_millis(180));
        }
    }

    fn ensure_export_active(&self, project_id: &str, task_id: &str) -> AppResult<()> {
        let task = self.repository.video_export_task(project_id, task_id)?;
        if task["status"].as_str() == Some(GENERATING) {
            return Ok(());
        }
        Err(AppError::BadRequest("视频打包已取消".to_owned()))
    }
}

fn export_sources(snapshot: &Value) -> AppResult<Vec<ExportSource>> {
    let sources = snapshot["selections"]
        .as_array()
        .ok_or_else(|| AppError::BadRequest("视频导出任务缺少分镜版本".to_owned()))?;
    if sources.is_empty() {
        return Err(AppError::BadRequest("没有可导出的视频版本".to_owned()));
    }
    sources
        .iter()
        .map(|source| {
            let video_url = source["video_url"].as_str().unwrap_or_default().trim();
            if video_url.is_empty() {
                return Err(AppError::BadRequest(
                    "视频导出任务包含空媒体地址".to_owned(),
                ));
            }
            Ok(ExportSource {
                episode_id: source["episode_id"].as_str().unwrap_or_default().to_owned(),
                episode_name: source["episode_name"]
                    .as_str()
                    .unwrap_or("未命名剧集")
                    .to_owned(),
                episode_order: source["episode_sort_order"].as_i64().unwrap_or(1),
                video_url: video_url.to_owned(),
            })
        })
        .collect()
}

fn group_by_episode(
    sources: Vec<ExportSource>,
    paths: Vec<PathBuf>,
) -> BTreeMap<(i64, String, String), Vec<PathBuf>> {
    sources
        .into_iter()
        .zip(paths)
        .fold(BTreeMap::new(), |mut grouped, (source, path)| {
            grouped
                .entry((source.episode_order, source.episode_name, source.episode_id))
                .or_insert_with(Vec::new)
                .push(path);
            grouped
        })
}

fn ffmpeg_program() -> &'static str {
    [
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
        "ffmpeg",
    ]
    .into_iter()
    .find(|program| *program == "ffmpeg" || Path::new(program).is_file())
    .unwrap_or("ffmpeg")
}

fn escape_concat_path(path: &Path) -> String {
    path.display().to_string().replace('\'', "'\\\\''")
}

fn safe_name(value: &str) -> String {
    let name = value
        .trim()
        .chars()
        .filter(|character| {
            !matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            )
        })
        .take(80)
        .collect::<String>();
    if name.is_empty() {
        "短剧".to_owned()
    } else {
        name
    }
}
