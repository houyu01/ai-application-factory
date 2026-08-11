//! Durable video-provider execution for individual interactive-game nodes.

use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    providers::VideoJob,
    repository::game_validation::GAME_VIDEO_DURATION_RANGE,
    value::{ground_game_video_prompt, now, SUCCEEDED},
};

use super::DurableWorker;

impl DurableWorker {
    /// Submit or poll a node's frozen prompt and references, preserving the queued task until the provider returns a playable URL.
    pub(super) fn game_video(&self, id: &str, game_id: &str, task: &Value) -> AppResult<()> {
        let node_id = task["resource_id"].as_str().unwrap_or_default();
        let game = self.repository.get_game(game_id)?;
        let node = self.repository.get_game_node(game_id, node_id)?;
        let original_prompt = task["input_snapshot"]["prompt"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| node["prompt"].as_str().unwrap_or_default());
        let original_prompt = ground_game_video_prompt(
            original_prompt,
            node["original_text"].as_str().unwrap_or_default(),
        );
        let refinement = task["input_snapshot"]["refinement"].as_object();
        let prompt = refinement.map_or_else(
            || original_prompt.to_owned(),
            |details| {
                let request = details
                    .get("prompt")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim();
                format!(
                    "原始提示词（仅供微调参考；未提及的内容请保持不变）：\n{original_prompt}\n\n用户微调提示词（必须优先执行）：\n{request}"
                )
            },
        );
        let mut references = task["input_snapshot"]["reference_images"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|asset| asset["image_url"].as_str())
            .filter_map(|url| self.media.provider_reference_url(url))
            .collect::<Vec<_>>();
        let voice_catalog = self.repository.voices()?;
        let mut reference_audio = task["input_snapshot"]["reference_images"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|asset| {
                asset["voice_id"]
                    .as_str()
                    .and_then(|voice_id| voice_catalog.iter().find(|voice| voice["id"] == voice_id))
                    .and_then(|voice| voice["audio_url"].as_str())
                    .and_then(|url| self.media.provider_reference_url(url))
            })
            .collect::<Vec<_>>();
        let mut prompt = append_voice_reference_notice(
            &prompt,
            &task["input_snapshot"]["reference_images"],
            &voice_catalog,
        );
        if let Some(frame) = task["input_snapshot"]["serial_first_frame"]
            .as_str()
            .filter(|value| !value.is_empty())
        {
            let first_frame = self.media.provider_reference_url(frame).ok_or_else(|| {
                AppError::BadRequest(
                    "上一节点视频尾帧无法发送给视频模型，请稍后重试串行生成".to_owned(),
                )
            })?;
            references.insert(0, first_frame);
            reference_audio.insert(0, None);
            prompt.push_str("\n\n串行连续性（最高优先级）：@图1 是上一视频节点的尾帧；本节点第一帧必须从该图的主体、构图、光线和状态自然衔接。");
        }
        let mut boundary_notes = Vec::new();
        for frame in task["input_snapshot"]["frame_images"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let side = frame["side"].as_str().unwrap_or_default();
            let raw = frame["url"].as_str().unwrap_or_default();
            if raw.is_empty() || !matches!(side, "first" | "last") {
                continue;
            }
            let reference = if raw.starts_with("data:image/") {
                self.media
                    .save_data_url(raw)
                    .ok()
                    .and_then(|url| self.media.provider_reference_url(&url))
            } else {
                self.media.provider_reference_url(raw)
            }
            .ok_or_else(|| {
                AppError::BadRequest(format!(
                    "所选{}帧无法发送给视频模型，请重新选择。",
                    if side == "first" { "首" } else { "尾" }
                ))
            })?;
            references.push(reference);
            reference_audio.push(None);
            boundary_notes.push(format!(
                "@图{} 是输入{}帧；生成视频的{}帧必须复现该图的主体、构图、光线和状态。",
                references.len(),
                if side == "first" { "首" } else { "尾" },
                if side == "first" { "首" } else { "尾" },
            ));
        }
        if !boundary_notes.is_empty() {
            prompt.push_str("\n\n首尾帧约束（高优先级）：");
            prompt.push_str(&boundary_notes.join("\n"));
        }
        let source_video = refinement
            .and_then(|details| details.get("source_video_url"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(|url| {
                self.media.provider_reference_url(url).ok_or_else(|| {
                    AppError::BadRequest(
                        "原始视频无法发送给当前视频模型，请检查媒体存储配置".to_owned(),
                    )
                })
            })
            .transpose()?;
        let result = if let Some(provider_task_id) = task["provider_task_id"]
            .as_str()
            .filter(|value| !value.is_empty())
        {
            self.providers.poll_video(provider_task_id)?
        } else {
            self.repository
                .update_game_task_progress(id, 8, "正在提交节点视频生成")?;
            self.providers.start_video(
                &prompt,
                game_video_ratio(&game),
                game["resolution"].as_str().unwrap_or("720p"),
                node["duration_seconds"].as_i64().unwrap_or(10).clamp(
                    *GAME_VIDEO_DURATION_RANGE.start(),
                    *GAME_VIDEO_DURATION_RANGE.end(),
                ),
                &references,
                &reference_audio,
                source_video.as_deref(),
                game["video_model"].as_str(),
            )?
        };
        let url = match result {
            VideoJob::Ready(url) => url,
            VideoJob::Pending {
                provider_task_id,
                progress,
            } => {
                self.repository.schedule_game_provider_poll(
                    id,
                    &provider_task_id,
                    progress,
                    "正在等待节点视频生成结果",
                )?;
                return Ok(());
            }
        };
        self.repository.finish_game_node_video(
            game_id,
            node_id,
            id,
            Some(&url),
            SUCCEEDED,
            None,
        )?;
        self.repository.finish_game_task(
            id,
            SUCCEEDED,
            Some(json!({"node_id":node_id,"id":id,"url":url,"task_id":id,"generated_at":now()})),
            None,
        )?;
        Ok(())
    }
}

fn append_voice_reference_notice(prompt: &str, references: &Value, voices: &[Value]) -> String {
    let lines = references
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|asset| {
            let voice = voices
                .iter()
                .find(|voice| voice["id"] == asset["voice_id"])?;
            voice["audio_url"].as_str().filter(|url| !url.is_empty())?;
            Some(format!(
                "角色音色：{}使用{}；已附带该角色音源参考（如模型支持）。",
                asset["name"].as_str().unwrap_or("角色"),
                voice["name"].as_str().unwrap_or("音色")
            ))
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        prompt.to_owned()
    } else {
        format!("{prompt}\n\n{}", lines.join("\n"))
    }
}

fn game_video_ratio(game: &Value) -> &'static str {
    if game["platform"].as_str() == Some("Steam游戏") {
        "16:9"
    } else {
        "9:16"
    }
}
