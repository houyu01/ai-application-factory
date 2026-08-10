//! Video-refinement service flow: save feedback on one history version and enqueue a dependent durable run.

use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    repository::{ShotVersionInput, ShotVideoRefinement},
    value::GENERATING,
};

use super::{video_snapshot, DesktopService};

impl DesktopService {
    /// The storyboard's “视频微调” dialog calls this flow to retain feedback on the selected video and create its next history version.
    pub fn enqueue_shot_video_refinement(
        &self,
        project_id: &str,
        shot_id: &str,
        source_version_id: &str,
        refinement_prompt: &str,
    ) -> AppResult<Value> {
        let refinement_prompt = refinement_prompt.trim();
        if refinement_prompt.is_empty() {
            return Err(AppError::BadRequest("请填写微调提示词".to_owned()));
        }
        if refinement_prompt.chars().count() > 4_000 {
            return Err(AppError::BadRequest(
                "微调提示词不能超过 4000 个字".to_owned(),
            ));
        }
        let project = self.repository.get_drama(project_id)?;
        let shot = self.repository.get_shot(project_id, shot_id)?;
        let source = self
            .repository
            .get_shot_version(project_id, shot_id, source_version_id)?;
        let source_video_url = source["video_url"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::BadRequest("只能微调已生成完成的视频".to_owned()))?
            .to_owned();
        let original_prompt = source["prompt"]
            .as_str()
            .filter(|value| !value.is_empty())
            .or_else(|| shot["prompt"].as_str())
            .unwrap_or_default()
            .to_owned();
        if original_prompt.trim().is_empty() {
            return Err(AppError::BadRequest(
                "所选视频缺少原始提示词，无法微调".to_owned(),
            ));
        }
        let original_prompt_rich = if source["prompt_rich"].is_array()
            && !source["prompt_rich"].as_array().is_some_and(Vec::is_empty)
        {
            source["prompt_rich"].clone()
        } else {
            video_snapshot::prompt_rich(&project, &shot)
        };
        let original_structured = if source["structured"].is_object()
            && !source["structured"]
                .as_object()
                .is_some_and(|value| value.is_empty())
        {
            source["structured"].clone()
        } else {
            shot["structured"].clone()
        };
        self.repository.set_shot_version_refinement_prompt(
            project_id,
            shot_id,
            source_version_id,
            refinement_prompt,
        )?;
        let active = self
            .repository
            .active_drama_tasks(project_id, "shot_video", Some(shot_id))?;
        if let Some(task) = active.into_iter().next() {
            return Ok(task);
        }
        self.repository
            .set_shot_status(project_id, shot_id, GENERATING)?;
        let task = self.repository.create_parallel_drama_task(
            project_id,
            "shot_video",
            Some(shot_id),
            json!({"project_id":project_id,"shot_id":shot_id}),
        )?;
        let task_id = task["id"].as_str().unwrap_or_default();
        let version = self.repository.create_shot_version_with_input(
            project_id,
            shot_id,
            task_id,
            ShotVersionInput {
                prompt: original_prompt.clone(),
                prompt_rich: original_prompt_rich.clone(),
                structured: original_structured.clone(),
                refinement: Some(ShotVideoRefinement {
                    source_version_id: source_version_id.to_owned(),
                    source_video_url: source_video_url.clone(),
                }),
            },
        )?;
        self.repository.update_drama_task_snapshot(
            task_id,
            json!({
                "project_id": project_id,
                "shot_id": shot_id,
                "version_id": version["id"],
                "refinement": {
                    "source_version_id": source_version_id,
                    "source_video_url": source_video_url,
                    "original_prompt": original_prompt,
                    "original_prompt_rich": original_prompt_rich,
                    "original_structured": original_structured,
                    "prompt": refinement_prompt,
                },
            }),
        )?;
        self.repository.get_drama_task(task_id)
    }
}
