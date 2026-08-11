//! Shared interactive-game material-image prompt assembly.

use serde_json::Value;

const CHARACTER_SHEET_PROMPT: &str = "生成完整角色设定板（character turnaround and expression sheet），规整多格排版；不要左右二分构图，不要只生成头像和单张全身像。第一排放同一角色三视图：正面、严格侧面、背面，均为从头到鞋子的全身站立视图；第二排六个等尺寸的表情特写：自然、微笑、悲伤、惊讶、生气、委屈；第三排四个全身动作：行走、奔跑或抬手、开心互动、害羞遮脸。所有格子严格服从当前素材提示词指定的角色形态；同一张图内保持同一张脸、该形态对应的年龄、发型、妆容、体型、服装和配饰，禁止把幼年、成年或其他形态混在一张图中；灰色摄影棚背景，柔和均匀布光，边界清晰，人物不重叠、不裁切、不变形，无文字、水印或多余人物。";

/// Combine global, public, base, and variant constraints for one reusable game material image.
pub(super) fn game_asset_generation_prompt(
    game: &Value,
    asset: &Value,
    variant: Option<&Value>,
) -> String {
    let kind = asset["type"].as_str().unwrap_or("prop");
    let public = game["asset_public_prompts"][kind]
        .as_str()
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .unwrap_or_else(|| default_game_asset_prompt(kind));
    let source = asset["prompt"].as_str().unwrap_or_default().trim();
    let variant_prompt = variant
        .and_then(|item| item["prompt"].as_str())
        .map(str::trim)
        .unwrap_or_default();
    [
        format!(
            "整体图片生成风格采用「{}」，为互动游戏保留可复用、一致的视觉设定。",
            game["style"].as_str().unwrap_or("真人风格")
        ),
        public.to_owned(),
        source.to_owned(),
        variant_prompt.to_owned(),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join("\n\n")
}

fn default_game_asset_prompt(kind: &str) -> &'static str {
    match kind {
        "character" => CHARACTER_SHEET_PROMPT,
        "scene" => "场景设定图：明确空间结构、时间氛围、关键光源和可供角色活动的区域；不要出现文字、水印或 UI。",
        _ => "道具设定图：完整展示道具轮廓、材质、尺寸关系和关键细节；背景干净，便于后续镜头反复引用。",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::game_asset_generation_prompt;

    #[test]
    fn default_character_prompt_requires_a_thirteen_panel_design_sheet() {
        let prompt = game_asset_generation_prompt(
            &json!({"style":"2D动漫风","asset_public_prompts":{}}),
            &json!({"type":"character","prompt":"年轻侦探，风衣与短发。"}),
            None,
        );

        for required in ["三视图", "六个等尺寸的表情", "第三排四个全身动作"] {
            assert!(prompt.contains(required), "missing {required}");
        }
        assert!(prompt.contains("整体图片生成风格采用「2D动漫风」"));
    }
}
