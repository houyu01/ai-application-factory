//! Provider-facing video prompts, visual references, and local boundary-frame conversion.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::{
    error::{AppError, AppResult},
    planner,
};

use super::{
    video_reference_lock::{first_frame_instruction, prioritize_first_frame},
    DurableWorker,
};

const REFERENCE_NOTICE: &str =
    "生成视频中所有的参考图，均为seedream生成的图片，并不是真人，请认真审核查看";
const CONTINUITY_NOTICE: &str = "视频全程保持画面内所有物体、道具、摆件数量不变，物体不消失、不凭空新增，物体位置轻微变化，物体形态材质保持一致，镜头平滑运动，无物体闪烁，无物体突然出现或突然消失，时序连贯，画面一致性强，流畅过渡。所有道具，尤其是手持、佩戴、携带或与人物接触的道具，必须保持同一实例的连续性；拿起、放下、递交、佩戴、收起或取出等转移，必须逐帧展示手部与道具的接触及完整运动过程，前后状态可追溯。若分镜动作未明确说明道具转移，人物与道具的持有、佩戴、摆放关系必须从首到尾保持不变。禁止任何道具或人物在手中、身上或画面中突然出现、消失、替换、跳变或变形。";
const CHARACTER_INTRODUCTION_NOTICE: &str = "人物首次出场规则：仅当分镜含“【人物首次出场”标记时，在该人物第一次清晰入画的最初1～2秒，于人物近旁快速显示其当前名字；姓名以该人物当前素材 name 为准，简洁可读、避开脸部、淡入淡出。姓名标识不是字幕，即使设置为不要字幕也必须保留；非首次出场禁止重复显示。";

impl DurableWorker {
    /// Build the exact provider payload inputs from persisted rich nodes, voices, and optional first/last frames.
    pub(super) fn video_generation_inputs(
        &self,
        project: &Value,
        shot: &Value,
    ) -> AppResult<(String, Vec<String>)> {
        let (asset_images, mut markers) = self.video_reference_plan(project, shot);
        let frames = self.video_boundary_frames(shot)?;
        let video_config = self.repository.setting("video")?;
        let provider = video_config["provider"].as_str().unwrap_or_default();
        let model = project["video_model"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase();
        let limit = (provider == "dashscope" && model.starts_with("wan2.7-r2v")).then_some(5_usize);
        let mut images = asset_images;
        if let Some(limit) = limit {
            images.truncate(limit.saturating_sub(frames.len()));
        }
        let selected_asset_count = images.len();
        let first_frame_index = frames
            .get("first")
            .map(|url| prioritize_first_frame(&mut images, &mut markers, url));
        let mut prompt = remap_markers(&self.video_prompt(project, shot)?, &markers);
        if selected_asset_count < asset_images_len(project, shot, &self.media) {
            prompt = omit_unselected_markers(&prompt, selected_asset_count);
        }
        let mut index_by_url = images
            .iter()
            .enumerate()
            .map(|(index, url)| (url.clone(), index + 1))
            .collect::<HashMap<_, _>>();
        let mut instructions = Vec::new();
        if let Some(index) = first_frame_index {
            instructions.push(first_frame_instruction(index));
        }
        if let Some(url) = frames.get("last") {
            let index = *index_by_url.entry(url.clone()).or_insert_with(|| {
                images.push(url.clone());
                images.len()
            });
            instructions.push(format!(
                "@图{index} 是视频尾帧：视频最后一帧必须收束到该图的主体、构图、光线和状态。"
            ));
        }
        if !instructions.is_empty() {
            prompt.push_str("\n\n首尾帧控制（最高优先级）：输入参考图与 @图编号按相同顺序对应。\n");
            prompt.push_str(&instructions.join("\n"));
        }
        Ok((prompt, images))
    }

    /// Render the image-generation policy shared by character, scene, prop, cover, and batch jobs.
    pub(super) fn asset_generation_prompt(&self, project: &Value, asset: &Value) -> String {
        let kind = asset["type"].as_str().unwrap_or("prop");
        let style = project["style"].as_str().unwrap_or("真人风格");
        let configured = project["asset_public_prompts"][kind]
            .as_str()
            .unwrap_or("")
            .trim();
        let public = if configured.is_empty() {
            default_asset_prompt(kind)
        } else {
            configured
        };
        let theme = asset_theme_constraint(project["theme"].as_str().unwrap_or("都市"), kind);
        let source = asset["prompt"].as_str().unwrap_or("").trim();
        let mut parts = vec![
            format!("整体图片生成风格采用「{style}」。"),
            public.to_owned(),
        ];
        if !source.contains(&theme) {
            parts.push(theme);
        }
        parts.push(source.to_owned());
        parts
            .into_iter()
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn video_prompt(&self, project: &Value, shot: &Value) -> AppResult<String> {
        let public = project["video_public_prompt"].as_str().unwrap_or("").trim();
        let mut public = if public.is_empty() {
            format!(
                "整体保持{}，题材为{}，按剧本处理方式组织镜头。",
                project["style"].as_str().unwrap_or("真人风格"),
                project["theme"].as_str().unwrap_or("都市")
            )
        } else {
            public.to_owned()
        };
        if !public.contains(CONTINUITY_NOTICE) {
            public.push('\n');
            public.push_str(CONTINUITY_NOTICE);
        }
        if !public.contains(CHARACTER_INTRODUCTION_NOTICE) {
            public.push('\n');
            public.push_str(CHARACTER_INTRODUCTION_NOTICE);
        }
        let constraints = &project["shot_constraints"];
        let constraint = format!(
            "分镜约束：{}；{}。",
            if constraints["subtitles"].as_bool().unwrap_or(false) {
                "需要字幕"
            } else {
                "不要字幕"
            },
            if constraints["background_music"].as_bool().unwrap_or(false) {
                "需要背景音乐"
            } else {
                "不要背景音乐"
            },
        );
        let mentioned = shot["prompt_rich"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|node| node["type"] == "reference")
            .filter_map(|node| node["asset_id"].as_str())
            .collect::<HashSet<_>>();
        let voices = self.repository.voices()?;
        let voice_lines = project["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|asset| asset["type"] == "character" && asset["voice_id"].is_string())
            .filter(|asset| {
                mentioned.is_empty()
                    || asset["id"]
                        .as_str()
                        .is_some_and(|id| mentioned.contains(id))
            })
            .filter_map(|asset| {
                let voice = voices
                    .iter()
                    .find(|voice| voice["id"] == asset["voice_id"])?;
                Some(format!(
                    "角色音色：{}使用{}；音色提示词：{}",
                    asset["name"].as_str().unwrap_or("角色"),
                    voice["name"].as_str().unwrap_or_default(),
                    voice["prompt"].as_str().unwrap_or_default(),
                ))
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok([
            public,
            REFERENCE_NOTICE.to_owned(),
            constraint,
            voice_lines,
            shot["prompt"].as_str().unwrap_or_default().to_owned(),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n"))
    }

    fn video_reference_plan(
        &self,
        project: &Value,
        shot: &Value,
    ) -> (Vec<String>, HashMap<i64, usize>) {
        let assets = project["assets"].as_array().cloned().unwrap_or_default();
        let mut images = Vec::new();
        let mut marker_indexes = HashMap::new();
        let mut asset_indexes = HashMap::new();
        let mut seen_urls = HashSet::new();
        for node in shot["prompt_rich"].as_array().into_iter().flatten() {
            if node["type"] != "reference" {
                continue;
            }
            let id = node["asset_id"].as_str().unwrap_or_default();
            let base_asset = assets.iter().find(|asset| asset["id"].as_str() == Some(id));
            if node["asset_type"].as_str() == Some("placeholder")
                && base_asset.and_then(|asset| asset["metadata"]["render_mode"].as_str())
                    != Some("generated_composite")
            {
                continue;
            }
            let asset = planner::resolve_reference_asset(&assets, id, node["variant_id"].as_str());
            let url = node["snapshot_image_url"]
                .as_str()
                .or_else(|| node["image_url"].as_str())
                .or_else(|| asset.as_ref().and_then(|asset| asset["image_url"].as_str()))
                .and_then(|url| self.media.provider_reference_url(url));
            let Some(url) = url else { continue };
            let key = planner::reference_key(id, node["variant_id"].as_str());
            let index = if let Some(index) = asset_indexes.get(&key) {
                *index
            } else if let Some(index) = images
                .iter()
                .position(|item| item == &url)
                .map(|item| item + 1)
            {
                asset_indexes.insert(key.clone(), index);
                index
            } else if seen_urls.insert(url.clone()) {
                images.push(url);
                let index = images.len();
                asset_indexes.insert(key, index);
                index
            } else {
                continue;
            };
            if let Some(marker) = node["mention_number"].as_i64() {
                marker_indexes.entry(marker).or_insert(index);
            }
        }
        (images, marker_indexes)
    }

    fn video_boundary_frames(&self, shot: &Value) -> AppResult<HashMap<String, String>> {
        let mut frames = HashMap::new();
        for side in ["first", "last"] {
            let value = &shot["first_last_frames"][side];
            let raw = value["url"]
                .as_str()
                .or_else(|| value.as_str())
                .unwrap_or("");
            if raw.is_empty() {
                continue;
            }
            let url = if raw.starts_with("data:image/") {
                self.media
                    .save_data_url(raw)
                    .ok()
                    .and_then(|saved| self.media.provider_reference_url(&saved))
            } else {
                self.media.provider_reference_url(raw)
            };
            let label = if side == "first" { "首帧" } else { "尾帧" };
            let url = url.ok_or_else(|| {
                AppError::BadRequest(format!(
                    "所选{label}图无法发送给视频模型，请重新选择或上传。"
                ))
            })?;
            frames.insert(side.to_owned(), url);
        }
        Ok(frames)
    }
}

fn asset_images_len(project: &Value, shot: &Value, media: &crate::media::MediaStore) -> usize {
    let mut urls = HashSet::new();
    for node in shot["prompt_rich"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|node| node["type"] == "reference")
    {
        let assets = project["assets"].as_array().cloned().unwrap_or_default();
        let base_asset = assets.iter().find(|asset| asset["id"] == node["asset_id"]);
        if node["asset_type"].as_str() == Some("placeholder")
            && base_asset.and_then(|asset| asset["metadata"]["render_mode"].as_str())
                != Some("generated_composite")
        {
            continue;
        }
        if let Some(url) = planner::resolve_reference_asset(
            &assets,
            node["asset_id"].as_str().unwrap_or_default(),
            node["variant_id"].as_str(),
        )
        .as_ref()
        .and_then(|asset| asset["image_url"].as_str())
        .or_else(|| node["image_url"].as_str())
        .and_then(|url| media.provider_reference_url(url))
        {
            urls.insert(url);
        }
    }
    urls.len()
}

fn remap_markers(prompt: &str, indexes: &HashMap<i64, usize>) -> String {
    replace_markers(prompt, |number, _| {
        format!(
            "@图{}",
            indexes.get(&number).copied().unwrap_or(number as usize)
        )
    })
}
fn omit_unselected_markers(prompt: &str, count: usize) -> String {
    replace_markers(prompt, |number, label| {
        if number as usize <= count {
            format!("@图{number}{label}")
        } else {
            label
                .trim_matches(['（', '）'])
                .to_owned()
                .if_empty("后续参考素材")
        }
    })
}

trait EmptyFallback {
    fn if_empty(self, fallback: &str) -> String;
}
impl EmptyFallback for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_owned()
        } else {
            self
        }
    }
}

fn replace_markers(prompt: &str, replace: impl Fn(i64, String) -> String) -> String {
    let chars = prompt.chars().collect::<Vec<_>>();
    let mut result = String::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != '@' || chars.get(index + 1) != Some(&'图') {
            result.push(chars[index]);
            index += 1;
            continue;
        }
        let start = index;
        index += 2;
        while chars
            .get(index)
            .is_some_and(|character| character.is_whitespace())
        {
            index += 1;
        }
        let digits = index;
        while chars.get(index).is_some_and(|value| value.is_ascii_digit()) {
            index += 1;
        }
        let Ok(number) = chars[digits..index]
            .iter()
            .collect::<String>()
            .parse::<i64>()
        else {
            result.push(chars[start]);
            index = start + 1;
            continue;
        };
        let label_start = index;
        if chars.get(index) == Some(&'（') {
            while chars.get(index).is_some_and(|value| *value != '）') {
                index += 1;
            }
            if chars.get(index) == Some(&'）') {
                index += 1;
            }
        }
        result.push_str(&replace(number, chars[label_start..index].iter().collect()));
    }
    result
}

fn default_asset_prompt(kind: &str) -> &'static str {
    match kind {
    "character" => "生成完整角色设定板（character turnaround and expression sheet），规整多格排版；不要左右二分构图，不要只生成头像和单张全身像。第一排放同一角色三视图：正面、严格侧面、背面，均为从头到鞋子的全身站立视图；第二排六个等尺寸的表情特写：自然、微笑、悲伤、惊讶、生气、委屈；第三排四个全身动作：行走、奔跑或抬手、开心互动、害羞遮脸。所有格子严格服从当前素材提示词指定的角色形态；同一张图内保持同一张脸、该形态对应的年龄、发型、妆容、体型、服装和配饰，禁止把幼年、成年或其他形态混在一张图中；灰色摄影棚背景，柔和均匀布光，边界清晰，人物不重叠、不裁切、不变形，无文字、水印或多余人物。",
    "scene" => "保持空间结构清晰、主体建筑或环境可识别，画面完整，适合作为短剧场景素材参考图。",
    "prop" => "主体道具清晰完整，材质、纹理和关键特征明确，画面干净，适合作为短剧道具素材参考图。",
    _ => "生成清晰、可复用的素材设定图。",
}
}

fn asset_theme_constraint(theme: &str, kind: &str) -> String {
    let focus = match kind {
        "character" => "角色身份、发型、妆容、服装与配饰",
        "scene" => "建筑、道路、室内陈设、照明、交通工具与环境细节",
        "prop" => "道具造型、材质、制作工艺、表面文字与实际功能",
        _ => "全部视觉元素",
    };
    format!("叙述背景主题：{}。{}必须符合该主题对应的时代、地域、社会环境与技术水平；除非剧本明确包含穿越或跨时代设定，否则禁止出现与背景主题不符的元素。", if theme.trim().is_empty() { "都市" } else { theme.trim() }, focus)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use crate::{db::Database, media::MediaStore, repository::Repository, value::new_id};

    use super::{DurableWorker, CHARACTER_INTRODUCTION_NOTICE, CONTINUITY_NOTICE};

    #[test]
    fn continuity_prompt_requires_visible_held_prop_transitions() {
        for text in [
            "手持",
            "逐帧展示手部与道具的接触",
            "持有、佩戴、摆放关系",
            "突然出现、消失、替换、跳变或变形",
        ] {
            assert!(CONTINUITY_NOTICE.contains(text), "missing {text}");
        }
        assert!(CHARACTER_INTRODUCTION_NOTICE.contains("当前名字"));
    }

    #[test]
    fn selected_first_frame_becomes_the_first_provider_reference_and_hard_prompt_rule() {
        let root = std::env::temp_dir().join(format!("video-first-frame-{}", new_id()));
        let repository = Repository::new(Database::open(root.join("test.db")).expect("database"));
        let media = MediaStore::new(repository.clone()).expect("media store");
        let scene = media
            .save_data_url("data:image/png;base64,iVBORw0KGgo=")
            .expect("scene image");
        let worker = DurableWorker::new(repository, media).expect("worker");
        let first = "data:image/png;base64,aGVsbG8=";
        let project = json!({"video_model":"doubao-seedance-2.0","style":"真人风格","theme":"都市","shot_constraints":{},"assets":[{"id":"scene","type":"scene","image_url":scene}]});
        let shot = json!({"prompt":"场景：@图1（旧居）","prompt_rich":[{"type":"reference","asset_id":"scene","asset_type":"scene","mention_number":1}],"first_last_frames":{"first":{"url":first}}});

        let (prompt, images) = worker
            .video_generation_inputs(&project, &shot)
            .expect("video inputs");

        assert_eq!(images.first().map(String::as_str), Some(first));
        assert!(prompt.contains("首帧图锁定（最高优先级，必须遵守）"));
        assert!(prompt.contains("@图1 是本分镜已明确选择的首帧图"));
        assert!(prompt.contains("场景：@图2"));
        assert!(prompt.contains("人物首次出场规则"));
        assert!(prompt.contains("禁止改用、替换、合并或凭空新增/删除任何人物、场景或道具"));
        fs::remove_dir_all(root).expect("remove test data");
    }
}
