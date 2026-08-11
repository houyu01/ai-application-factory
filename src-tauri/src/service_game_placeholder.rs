//! Interactive-game placeholder validation and durable composite-task creation.

use serde_json::{json, Map, Value};

use crate::error::{AppError, AppResult};

use super::DesktopService;

impl DesktopService {
    /// Save a game node's editable placeholder scene and character boxes without requesting an image provider.
    pub fn save_game_placeholder_layout(
        &self,
        game_id: &str,
        node_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        self.repository
            .save_game_placeholder_layout(game_id, node_id, values)
    }

    /// Validate a node composition, persist its draft, and create a restart-safe placeholder image task.
    pub fn enqueue_game_placeholder(
        &self,
        game_id: &str,
        node_id: &str,
        values: Map<String, Value>,
    ) -> AppResult<Value> {
        let node = self
            .repository
            .save_game_placeholder_layout(game_id, node_id, values)?;
        let game = self.repository.get_game(game_id)?;
        let scene_id = node["placeholder_scene_asset_id"]
            .as_str()
            .ok_or_else(|| AppError::BadRequest("请选择场景素材".to_owned()))?;
        let placements = node["placeholder_placements"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if placements.is_empty() {
            return Err(AppError::BadRequest(
                "请至少添加一个角色到占位图".to_owned(),
            ));
        }
        let assets = game["assets"].as_array().cloned().unwrap_or_default();
        let scene = asset(&assets, scene_id, "scene")?;
        require_image(&scene, "请先生成或上传场景图片，再创建占位图")?;
        let mut characters = Vec::new();
        for placement in &placements {
            let character = asset(
                &assets,
                placement["asset_id"].as_str().unwrap_or_default(),
                "character",
            )?;
            require_image(
                &character,
                &format!(
                    "角色“{}”尚未生成或上传图片",
                    character["name"].as_str().unwrap_or("未命名")
                ),
            )?;
            if !characters
                .iter()
                .any(|item: &Value| item["id"] == character["id"])
            {
                characters.push(character);
            }
        }
        let context = ["title", "original_text", "prompt"]
            .into_iter()
            .filter_map(|key| node[key].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let references = node["reference_asset_ids"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut props = Vec::new();
        for candidate in assets.iter().filter(|item| item["type"] == "prop") {
            let selected = references.iter().any(|id| id == &candidate["id"]);
            let mentioned = candidate["name"]
                .as_str()
                .is_some_and(|name| context.contains(name));
            if !(selected || mentioned) {
                continue;
            }
            if candidate["image_url"]
                .as_str()
                .is_some_and(|url| !url.is_empty())
            {
                props.push(candidate.clone());
            } else if selected {
                return Err(AppError::BadRequest(format!(
                    "道具“{}”尚未生成或上传图片",
                    candidate["name"].as_str().unwrap_or("未命名")
                )));
            }
        }
        let mut reference_ids = vec![scene_id.to_owned()];
        reference_ids.extend(
            characters
                .iter()
                .filter_map(|item| item["id"].as_str())
                .map(str::to_owned),
        );
        reference_ids.extend(
            props
                .iter()
                .filter_map(|item| item["id"].as_str())
                .map(str::to_owned),
        );
        let metadata = json!({
            "node_id":node_id,
            "scene_asset_id":scene_id,
            "scene_name":scene["name"],
            "placements":placements,
            "character_asset_ids":characters.iter().map(|item| item["id"].clone()).collect::<Vec<_>>(),
            "prop_asset_ids":props.iter().map(|item| item["id"].clone()).collect::<Vec<_>>(),
            "reference_asset_ids":reference_ids,
        });
        self.repository.enqueue_game_placeholder(
            game_id,
            node_id,
            &game_placeholder_prompt(&game, &node, &scene, &characters, &props, &metadata),
            metadata,
        )
    }
}

fn asset(assets: &[Value], id: &str, expected_type: &str) -> AppResult<Value> {
    assets
        .iter()
        .find(|item| item["id"].as_str() == Some(id))
        .filter(|item| item["type"].as_str() == Some(expected_type))
        .cloned()
        .ok_or_else(|| AppError::BadRequest(format!("占位图参考素材不存在或类型不匹配：{id}")))
}

fn require_image(asset: &Value, message: &str) -> AppResult<()> {
    if asset["image_url"]
        .as_str()
        .is_some_and(|url| !url.is_empty())
    {
        Ok(())
    } else {
        Err(AppError::BadRequest(message.to_owned()))
    }
}

fn game_placeholder_prompt(
    game: &Value,
    node: &Value,
    scene: &Value,
    characters: &[Value],
    props: &[Value],
    metadata: &Value,
) -> String {
    let positions = metadata["placements"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, placement)| {
            let role = characters
                .iter()
                .find(|role| role["id"] == placement["asset_id"])
                .unwrap_or(&Value::Null);
            let horizontal = if placement["x"].as_f64().unwrap_or(0.5) < 0.34 {
                "左侧"
            } else if placement["x"].as_f64().unwrap_or(0.5) > 0.66 {
                "右侧"
            } else {
                "中央"
            };
            let vertical = if placement["y"].as_f64().unwrap_or(0.5) > 0.5 {
                "前景"
            } else if placement["y"].as_f64().unwrap_or(0.5) > 0.22 {
                "中景"
            } else {
                "后景"
            };
            format!(
                "参考图{}中的角色“{}”位于画面{}{}，相对位置 x={:.2}, y={:.2}，画面占比宽={:.2}, 高={:.2}；动作/备注：{}",
                index + 2,
                role["name"].as_str().unwrap_or("角色"),
                horizontal,
                vertical,
                placement["x"].as_f64().unwrap_or(0.28),
                placement["y"].as_f64().unwrap_or(0.26),
                placement["width"].as_f64().unwrap_or(0.2),
                placement["height"].as_f64().unwrap_or(0.35),
                placement["note"].as_str().unwrap_or("站立"),
            )
        })
        .collect::<Vec<_>>();
    let ratio = if game["platform"].as_str() == Some("Steam游戏") {
        "16:9"
    } else {
        "9:16"
    };
    [
        vec![
            "生成一张干净、完整、可直接提供给视频生成模型的互动游戏节点构图参考图。".to_owned(),
            format!(
                "节点：{}；节点文本：{}",
                node["title"].as_str().unwrap_or("未命名节点"),
                node["original_text"].as_str().unwrap_or("")
            ),
            format!(
                "场景：{}；场景提示词：{}",
                scene["name"].as_str().unwrap_or("未命名场景"),
                scene["prompt"].as_str().unwrap_or("")
            ),
            format!("风格：{}；画幅：{ratio}", game["style"].as_str().unwrap_or("真人风格")),
            "参考图1是场景；后续参考图是角色和剧情相关道具。保持参考人物脸部、服装、道具材质与场景结构一致。".to_owned(),
        ],
        positions,
        props
            .iter()
            .map(|item| {
                format!(
                    "道具“{}”：{}",
                    item["name"].as_str().unwrap_or(""),
                    item["prompt"].as_str().unwrap_or("")
                )
            })
            .collect(),
        vec![
            "将角色和道具自然融合进场景并符合透视、遮挡、光影和比例关系。".to_owned(),
            "输出必须是无编辑痕迹的成片参考图：不要方框、字母、标签、箭头、辅助线、坐标、界面、文字或水印。".to_owned(),
        ],
    ]
    .concat()
    .join("\n")
}
