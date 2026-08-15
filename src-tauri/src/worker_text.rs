//! Language-model and deterministic text tasks for short-drama workflows.

use serde_json::json;

use crate::{error::AppResult, planner, value::SUCCEEDED};

use super::{prompt_helpers::*, DurableWorker};

impl DurableWorker {
    pub(super) fn decompose(&self, id: &str, project_id: &str) -> AppResult<()> {
        self.repository
            .update_drama_task_progress(id, 10, "正在准备剧本")?;
        let raw = self.repository.raw_drama(project_id)?;
        let script = raw["script"].as_str().unwrap_or_default();
        let screenplay = self.prepare_screenplay_for_decomposition(id, project_id, &raw)?;
        self.repository
            .update_drama_task_progress(id, 65, "正在拆解扩写剧本")?;
        let mut plan = self.long_form_plan(id, &raw, &screenplay)?;
        self.repository
            .update_drama_task_progress(id, 75, "正在让模型复核人物、场景和道具目录")?;
        let reviewed_assets = self.review_asset_catalog_with_model(
            &raw,
            &screenplay,
            plan["assets"].as_array().cloned().unwrap_or_default(),
        );
        plan["assets"] = json!(reviewed_assets);
        super::decomposition_assets::enrich(
            &mut plan,
            raw["theme"].as_str().unwrap_or("都市"),
            raw["style"].as_str().unwrap_or("真人风格"),
        );
        self.repository
            .update_drama_task_progress(id, 75, "正在整理分集、分镜和素材")?;
        self.ensure_expansion_active(id)?;
        self.repository
            .set_expanded_screenplay(project_id, &screenplay)?;
        self.repository
            .save_drama_decomposition(project_id, &plan)?;
        self.repository.finish_drama_task(
            id,
            SUCCEEDED,
            Some(json!({
                "original_script_length":script.chars().count(),
                "expanded_script_length":screenplay.chars().count()
            })),
            None,
        )?;
        Ok(())
    }

    pub(super) fn expand(&self, id: &str, project_id: &str) -> AppResult<()> {
        self.continue_expanded_screenplay(id, project_id)
    }

    pub(super) fn shot_prompt(&self, id: &str, project_id: &str, shot_id: &str) -> AppResult<()> {
        let project = self.repository.get_drama(project_id)?;
        let shot = self.repository.get_shot(project_id, shot_id)?;
        let assets = project["assets"].as_array().cloned().unwrap_or_default();
        let references = selected_ready_references(&shot, &assets);
        let requested_version = shot["prompt_template_version"].as_str().unwrap_or("v1");
        let template = self
            .repository
            .prompt_templates("drama", Some("shot_prompt"), false)?
            .into_iter()
            .find(|item| item["version"].as_str() == Some(requested_version));
        let template = template.or_else(|| {
            self.repository
                .prompt_templates("drama", Some("shot_prompt"), false)
                .ok()
                .and_then(|items| items.into_iter().next())
        });
        let version = template
            .as_ref()
            .and_then(|item| item["version"].as_str())
            .unwrap_or(requested_version);
        let system = template
            .as_ref()
            .and_then(|item| item["template_text"].as_str())
            .unwrap_or("返回可编辑分镜富提示词 JSON。");
        let enable_web_search = crate::value::bool_value(&project["enable_web_search"]);
        let mut nodes = self
            .providers
            .complete_with_web_search(
                "language",
                project["language_model"].as_str(),
                system,
                &rich_prompt_request(&project, &shot, &references, version),
                enable_web_search,
            )?
            .and_then(|response| planner::model_rich_prompt(&response, &references))
            .unwrap_or_else(|| planner::fallback_rich_prompt(&project, &shot, &references));
        nodes = append_missing_references(nodes, &references);
        nodes = filter_disallowed_sections(nodes, &project);
        let prompt = planner::prompt_text(&nodes);
        let reference_ids = nodes
            .iter()
            .filter_map(|node| node["asset_id"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        let structured = structured_from_prompt(&project, &shot, &nodes);
        self.repository.save_generated_shot_prompt(
            project_id,
            shot_id,
            &prompt,
            &nodes,
            &structured,
            &reference_ids,
            template.as_ref().and_then(|item| item["id"].as_str()),
            version,
        )?;
        let quality = self.repository.create_active_drama_task(
            project_id,
            "shot_quality",
            Some(shot_id),
            json!({"project_id":project_id,"shot_id":shot_id}),
        )?;
        self.repository
            .mark_shot_quality_pending(project_id, shot_id)?;
        self.repository
            .finish_drama_task(id, SUCCEEDED, Some(json!({"shot_id":shot_id,"prompt":prompt,"prompt_rich":nodes,"quality_task_id":quality["id"]})), None)?;
        Ok(())
    }

    pub(super) fn shot_quality(&self, id: &str, project_id: &str, shot_id: &str) -> AppResult<()> {
        let project = self.repository.get_drama(project_id)?;
        let shot = self.repository.get_shot(project_id, shot_id)?;
        let assets = project["assets"].as_array().cloned().unwrap_or_default();
        let nodes = shot["prompt_rich"].as_array().cloned().unwrap_or_default();
        let structured = if shot["structured"]
            .as_object()
            .is_some_and(|item| !item.is_empty())
        {
            shot["structured"].clone()
        } else {
            structured_from_prompt(&project, &shot, &nodes)
        };
        let prompt = shot["prompt"].as_str().unwrap_or_default();
        let mut issues = Vec::new();
        if prompt.trim().is_empty() || nodes.is_empty() {
            issues.push(issue(
                "EMPTY_PROMPT",
                "error",
                "分镜富文本提示词为空",
                "prompt",
            ));
        }
        if prompt.trim() == shot["original_text"].as_str().unwrap_or_default().trim() {
            issues.push(issue(
                "UNSPLIT_SOURCE_TEXT",
                "error",
                "提示词仍是整段原始剧本，尚未拆解为分镜结构",
                "prompt",
            ));
        }
        if !nodes.iter().any(|node| {
            node["type"] == "text"
                && node["text"]
                    .as_str()
                    .is_some_and(|text| !text.trim().is_empty())
        }) {
            issues.push(issue(
                "NO_TEXT_NODE",
                "error",
                "富文本提示词缺少文字描述",
                "prompt_rich",
            ));
        }
        if !nodes.iter().any(|node| node["type"] == "reference") {
            issues.push(issue(
                "NO_REFERENCE",
                "warning",
                "提示词尚未引用角色、场景、道具或占位图",
                "references",
            ));
        }
        for node in nodes.iter().filter(|node| node["type"] == "reference") {
            let asset = planner::resolve_reference_asset(
                &assets,
                node["asset_id"].as_str().unwrap_or_default(),
                node["variant_id"].as_str(),
            );
            match asset {
                None => issues.push(issue(
                    "MISSING_ASSET",
                    "error",
                    &format!(
                        "引用素材不存在：{}",
                        node["label"].as_str().unwrap_or("未命名")
                    ),
                    "references",
                )),
                Some(asset) if asset["status"].as_str() != Some(SUCCEEDED) => issues.push(issue(
                    "ASSET_NOT_READY",
                    "error",
                    &format!(
                        "素材尚未生成成功：{}",
                        asset["name"].as_str().unwrap_or("未命名")
                    ),
                    "references",
                )),
                Some(asset) if asset["image_url"].as_str().is_none_or(str::is_empty) => issues
                    .push(issue(
                        "MISSING_IMAGE",
                        "error",
                        &format!(
                            "素材尚未生成图片：{}",
                            asset["name"].as_str().unwrap_or("未命名")
                        ),
                        "references",
                    )),
                _ => {}
            }
        }
        if structured["scene_reference_ids"]
            .as_array()
            .is_none_or(|items| items.is_empty())
        {
            issues.push(issue(
                "MISSING_SCENE",
                "warning",
                "分镜没有场景参考图",
                "scene_reference_ids",
            ));
        }
        let cameras = structured["camera_shots"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if cameras.is_empty() {
            issues.push(issue(
                "MISSING_CAMERA",
                "error",
                "没有检测到镜头结构",
                "camera_shots",
            ));
        }
        if cameras
            .iter()
            .any(|camera| camera["description"].as_str().is_none_or(str::is_empty))
        {
            issues.push(issue(
                "EMPTY_CAMERA_DESCRIPTION",
                "error",
                "存在没有动作描述的镜头",
                "camera_shots",
            ));
        }
        for (key, label) in [
            ("scene", "场景"),
            ("style", "风格"),
            ("lighting", "光线"),
            ("position", "位置"),
        ] {
            if !structured["sections"][key].as_bool().unwrap_or(false) {
                issues.push(issue(
                    &format!("MISSING_{}", key.to_uppercase()),
                    "warning",
                    &format!("提示词缺少{label}结构段落"),
                    "prompt",
                ));
            }
        }
        let voice_count = structured["voice_blocks"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0);
        if cameras.len() > 1 && voice_count != 0 && voice_count != cameras.len() {
            issues.push(issue(
                "VOICE_SHOT_MISMATCH",
                "warning",
                "配音段落数量与镜头数量不一致",
                "voice_blocks",
            ));
        }
        if !project["shot_constraints"]["subtitles"]
            .as_bool()
            .unwrap_or(false)
            && prompt.contains("字幕")
        {
            issues.push(issue(
                "SUBTITLE_CONSTRAINT",
                "error",
                "项目禁止字幕，但提示词包含字幕描述",
                "prompt",
            ));
        }
        if !project["shot_constraints"]["background_music"]
            .as_bool()
            .unwrap_or(false)
            && ["背景音乐", "配乐"]
                .iter()
                .any(|term| prompt.contains(term))
        {
            issues.push(issue(
                "MUSIC_CONSTRAINT",
                "error",
                "项目禁止背景音乐，但提示词包含音乐描述",
                "prompt",
            ));
        }
        if ["http://", "https://", "data:image", "asset://", "tos://"]
            .iter()
            .any(|term| prompt.contains(term))
        {
            issues.push(issue(
                "TECHNICAL_REFERENCE",
                "error",
                "提示词不能包含图片 URL 或技术标识",
                "prompt",
            ));
        }
        let errors = issues
            .iter()
            .filter(|item| item["severity"] == "error")
            .count() as i64;
        let warnings = issues
            .iter()
            .filter(|item| item["severity"] == "warning")
            .count() as i64;
        let quality = json!({"status":if errors == 0 { "通过" } else { "需修改" },"score":(100-errors*25-warnings*5).max(0),"issues":issues,"checks":{"references":!issues.iter().any(|item| ["MISSING_ASSET","ASSET_NOT_READY","MISSING_IMAGE"].contains(&item["code"].as_str().unwrap_or_default())),"camera":!issues.iter().any(|item| item["code"] == "MISSING_CAMERA"),"constraints":!issues.iter().any(|item| ["SUBTITLE_CONSTRAINT","MUSIC_CONSTRAINT"].contains(&item["code"].as_str().unwrap_or_default()))}});
        self.repository.update_shot(
            project_id,
            shot_id,
            serde_json::Map::from_iter([("structured".to_owned(), structured)]),
        )?;
        self.repository
            .set_shot_quality(project_id, shot_id, quality.clone())?;
        self.repository
            .finish_drama_task(id, SUCCEEDED, Some(quality), None)?;
        Ok(())
    }
}
