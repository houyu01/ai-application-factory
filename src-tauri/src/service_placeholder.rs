//! Placeholder-composite enqueue flow, including durable asset metadata and reference validation.

use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    value::GENERATING,
};

use super::DesktopService;

impl DesktopService {
    /// Persist a generated-composite asset before its model task so layout intent survives a restart.
    pub fn enqueue_placeholder(
        &self,
        project_id: &str,
        shot_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        let project = self.repository.get_drama(project_id)?;
        let shot = self.repository.get_shot(project_id, shot_id)?;
        let scene_id = string_value(&values, "scene_asset_id")?;
        let scene = self.repository.get_asset(project_id, &scene_id)?;
        if scene["type"].as_str() != Some("scene") {
            return Err(AppError::BadRequest(
                "占位图必须使用场景素材作为背景".to_owned(),
            ));
        }
        if empty(&scene["image_url"]) {
            return Err(AppError::BadRequest(
                "请先生成场景图片，再创建占位图".to_owned(),
            ));
        }
        let placements = normalized_placements(values.get("placements"))?;
        let assets = project["assets"].as_array().cloned().unwrap_or_default();
        let mut characters = Vec::new();
        for placement in &placements {
            let id = placement["asset_id"].as_str().unwrap_or_default();
            let asset = assets
                .iter()
                .find(|asset| asset["id"].as_str() == Some(id))
                .ok_or_else(|| AppError::NotFound(format!("Asset not found: {id}")))?;
            if asset["type"].as_str() != Some("character") {
                return Err(AppError::BadRequest("占位图只能放置角色素材".to_owned()));
            }
            if empty(&asset["image_url"]) {
                return Err(AppError::BadRequest(format!(
                    "角色“{}”尚未生成图片",
                    asset["name"].as_str().unwrap_or("未命名")
                )));
            }
            if !characters
                .iter()
                .any(|item: &Value| item["id"] == asset["id"])
            {
                characters.push(asset.clone());
            }
        }
        let context = ["title", "original_text", "prompt"]
            .into_iter()
            .filter_map(|key| shot[key].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let mut explicit = shot["reference_asset_ids"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        explicit.extend(
            shot["prompt_rich"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|node| node["type"] == "reference")
                .map(|node| node["asset_id"].clone()),
        );
        let mut props = Vec::new();
        for asset in &assets {
            if asset["type"].as_str() != Some("prop") {
                continue;
            }
            let selected = explicit.iter().any(|id| id == &asset["id"]);
            let mentioned = asset["name"]
                .as_str()
                .is_some_and(|name| context.contains(name));
            if !(selected || mentioned) {
                continue;
            }
            if empty(&asset["image_url"]) {
                if selected {
                    return Err(AppError::BadRequest(format!(
                        "道具“{}”尚未生成图片",
                        asset["name"].as_str().unwrap_or("未命名")
                    )));
                }
            } else {
                props.push(asset.clone());
            }
        }
        let mut references = vec![scene.clone()];
        references.extend(characters.clone());
        references.extend(props.clone());
        if let Some(mut active) = self.repository.active_drama_task_by_snapshot(
            project_id,
            "placeholder_image",
            "shot_id",
            shot_id,
        )? {
            active
                .as_object_mut()
                .expect("task is an object")
                .insert("_reused".to_owned(), json!(true));
            return Ok(active);
        }
        let count = assets
            .iter()
            .filter(|asset| {
                asset["type"] == "placeholder"
                    && asset["metadata"]["shot_id"] == shot_id
                    && asset["metadata"]["render_mode"] == "generated_composite"
            })
            .count()
            + 1;
        let reference_ids = references
            .iter()
            .filter_map(|asset| asset["id"].as_str())
            .collect::<Vec<_>>();
        let metadata = json!({"shot_id":shot_id,"scene_asset_id":scene_id,"scene_name":scene["name"],"placements":placements,"version":count,"render_mode":"generated_composite","character_asset_ids":characters.iter().map(|asset| asset["id"].clone()).collect::<Vec<_>>(),"prop_asset_ids":props.iter().map(|asset| asset["id"].clone()).collect::<Vec<_>>(),"reference_asset_ids":reference_ids});
        let asset = self.repository.create_asset(
            project_id,
            Map::from_iter([
                ("type".to_owned(), json!("placeholder")),
                (
                    "name".to_owned(),
                    json!(format!(
                        "{} · 占位图 {count}",
                        shot["title"].as_str().unwrap_or("分镜")
                    )),
                ),
                (
                    "prompt".to_owned(),
                    json!(placeholder_prompt(
                        &project,
                        &scene,
                        &characters,
                        &props,
                        &placements
                    )),
                ),
                ("metadata".to_owned(), metadata.clone()),
            ]),
        )?;
        self.repository.set_asset_status(
            project_id,
            asset["id"].as_str().unwrap_or_default(),
            GENERATING,
        )?;
        let mut task = self.repository.create_active_drama_task(project_id, "placeholder_image", asset["id"].as_str(), json!({"project_id":project_id,"shot_id":shot_id,"asset_id":asset["id"],"scene_asset_id":scene_id,"placements":placements,"reference_asset_ids":metadata["reference_asset_ids"],"render_mode":"generated_composite","type":"placeholder_image"}))?;
        task.as_object_mut()
            .expect("task is an object")
            .insert("_reused".to_owned(), json!(false));
        Ok(task)
    }
}

fn string_value(values: &Map<String, Value>, key: &str) -> AppResult<String> {
    values
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| AppError::BadRequest(format!("缺少 {key}")))
}
fn empty(value: &Value) -> bool {
    value.as_str().is_none_or(str::is_empty)
}
fn number(value: &Value, default: f64) -> f64 {
    value.as_f64().unwrap_or(default)
}
fn normalized_placements(raw: Option<&Value>) -> AppResult<Vec<Value>> {
    let mut result = Vec::new();
    for placement in raw.and_then(Value::as_array).into_iter().flatten() {
        let id = placement["asset_id"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::BadRequest("占位图位置缺少角色素材".to_owned()))?;
        let width = number(&placement["width"], 0.2).clamp(0.04, 1.0);
        let height = number(&placement["height"], 0.35).clamp(0.04, 1.0);
        let placement_id = placement["id"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("placement_{}", result.len() + 1));
        result.push(json!({"id":placement_id,"asset_id":id,"x":number(&placement["x"], 0.28).clamp(0.0,1.0-width),"y":number(&placement["y"], 0.26).clamp(0.0,1.0-height),"width":width,"height":height,"pose":placement["pose"].as_str().unwrap_or(""),"note":placement["note"].as_str().or_else(|| placement["pose"].as_str()).unwrap_or("")}));
    }
    if result.is_empty() {
        return Err(AppError::BadRequest(
            "请至少添加一个角色到占位图".to_owned(),
        ));
    }
    result.truncate(30);
    Ok(result)
}
fn placeholder_prompt(
    project: &Value,
    scene: &Value,
    characters: &[Value],
    props: &[Value],
    placements: &[Value],
) -> String {
    let positions = placements.iter().enumerate().map(|(index, placement)| { let role=characters.iter().find(|role| role["id"]==placement["asset_id"]).unwrap_or(&Value::Null); let horizontal=if placement["x"].as_f64().unwrap_or(0.5)<0.34{"左侧"}else if placement["x"].as_f64().unwrap_or(0.5)>0.66{"右侧"}else{"中央"}; let vertical=if placement["y"].as_f64().unwrap_or(0.5)>0.5{"前景"}else if placement["y"].as_f64().unwrap_or(0.5)>0.22{"中景"}else{"后景"}; format!("参考图{}中的角色“{}”位于画面{}{}，相对位置 x={:.2}, y={:.2}，画面占比宽={:.2}, 高={:.2}；动作/备注：{}",index+2,role["name"].as_str().unwrap_or("角色"),horizontal,vertical,placement["x"].as_f64().unwrap_or(0.28),placement["y"].as_f64().unwrap_or(0.26),placement["width"].as_f64().unwrap_or(0.2),placement["height"].as_f64().unwrap_or(0.35),placement["note"].as_str().unwrap_or("站立")) }).collect::<Vec<_>>();
    [vec!["生成一张干净、完整、可直接提供给视频生成模型的镜头构图参考图。".to_owned(),format!("场景：{}；场景提示词：{}",scene["name"].as_str().unwrap_or("未命名场景"),scene["prompt"].as_str().unwrap_or("")),format!("风格：{}；背景主题：{}；画幅：{}",project["style"].as_str().unwrap_or("真人风格"),project["theme"].as_str().unwrap_or(""),project["ratio"].as_str().unwrap_or("9:16")),"参考图1是场景；后续参考图是角色和剧情相关道具。保持参考人物脸部、服装、道具材质与场景结构一致。".to_owned()],positions,props.iter().map(|item|format!("道具“{}”：{}",item["name"].as_str().unwrap_or(""),item["prompt"].as_str().unwrap_or(""))).collect(),vec!["将角色和道具自然融合进场景并符合透视、遮挡、光影和比例关系。".to_owned(),"输出必须是无编辑痕迹的成片参考图：不要方框、字母、标签、箭头、辅助线、坐标、界面、文字或水印。".to_owned()]].concat().join("\n")
}
