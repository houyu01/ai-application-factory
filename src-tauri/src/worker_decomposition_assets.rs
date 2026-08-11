//! Asset-specific fallback descriptions and voice selection for the persisted storyboard plan.

use serde_json::{json, Value};

const VOICE_IDS: &[&str] = &[
    "broken_whisper_resilient_female",
    "cold_boss_male",
    "cool_career_newcomer_male",
    "soft_puppy_boyfriend_male",
    "sickly_gloomy_yandere_male",
    "ruthless_old_fox_male",
    "arrogant_genius_male",
    "cool_abstinent_detective_female",
    "warm_older_brother_male",
    "sweet_cold_yandere_male",
    "cold_royal_sister_female",
    "strong_female_lead",
    "mature_warm_goddess_female",
    "sweet_fox_tease_female",
];

/// Give every decomposed asset a durable private visual description and every character a usable voice.
///
/// The bootstrap worker calls this after the model returns its plan but before SQLite persistence.
/// It protects image generation from terse model output and deliberately never reads project public prompts.
pub(super) fn enrich(plan: &mut Value, theme: &str, style: &str) {
    let Some(assets) = plan["assets"].as_array_mut() else {
        return;
    };
    for (index, asset) in assets.iter_mut().enumerate() {
        let kind = asset["type"].as_str().unwrap_or("prop").to_owned();
        let name = asset["name"]
            .as_str()
            .unwrap_or("未命名素材")
            .trim()
            .to_owned();
        let source = visual_description(asset["prompt"].as_str().unwrap_or_default());
        asset["prompt"] = json!(private_prompt(&kind, &name, &source, theme, style, index));
        if kind == "character" {
            let voice = asset["voice_id"]
                .as_str()
                .filter(|id| VOICE_IDS.contains(id))
                .unwrap_or_else(|| matching_voice(&name, &source));
            asset["voice_id"] = json!(voice);
            enrich_variants(asset, &name, &source, theme, style);
        }
    }
}

fn private_prompt(
    kind: &str,
    name: &str,
    source: &str,
    theme: &str,
    style: &str,
    index: usize,
) -> String {
    let source = if source.is_empty() {
        fallback_description(kind, name, index)
    } else {
        source.to_owned()
    };
    match kind {
        "character" => format!(
            "叙述背景主题：{theme}\n风格：{style}\n角色身份与性格：{source}\n外观设定：{}\n连续性要求：基础形态的发型、脸部特征、身型、服装层次和随身配饰在同一形态的后续镜头中保持一致；如剧本列有其他年龄、状态或换装形态，必须改用对应形态参考图，不能与基础形态混用；呈现{style}视觉细节，无文字水印。",
            character_appearance(index, is_female(name, &source))
        ),
        "scene" => format!(
            "叙述背景主题：{theme}\n风格：{style}\n场景名称与剧情用途：{name}，{source}\n空间与主体：{}\n陈设与氛围：场内物件带有真实使用状态，色调、主光和空气感服务于剧情；无人物、无背景文字、无水印。",
            scene_structure(index)
        ),
        _ => format!(
            "叙述背景主题：{theme}\n风格：{style}\n道具名称与叙事用途：{name}，{source}\n外观细节：{}\n呈现限制：单一主体清晰完整，干净静物构图，材质纹理和磨损可辨，无品牌、无多余文字、无水印。",
            prop_details(index)
        ),
    }
}

/// Remove global headers emitted by the model so the persisted template owns theme and style exactly once.
fn visual_description(source: &str) -> String {
    source
        .lines()
        .filter(|line| {
            let line = line.trim();
            !line.starts_with("叙述背景主题：") && !line.starts_with("风格：")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

/// Preserve every model-extracted character form as an independently usable image prompt.
fn enrich_variants(asset: &mut Value, name: &str, identity: &str, theme: &str, style: &str) {
    let Some(variants) = asset["variants"].as_array_mut() else {
        return;
    };
    for variant in variants {
        let form_name = variant["name"].as_str().unwrap_or("其他形态").trim();
        let form_source = visual_description(variant["prompt"].as_str().unwrap_or_default());
        let source = if form_source.is_empty() {
            format!("{form_name}，外观必须与剧本明确的阶段变化一致。")
        } else {
            form_source.to_owned()
        };
        variant["prompt"] = json!(format!(
            "叙述背景主题：{theme}\n风格：{style}\n角色身份锚点：{name}。{identity}\n当前形态：{form_name}。{source}\n形态绘制要求：清楚表现该阶段的年龄/生命时期、脸型和体态比例、发型、服装层次、配饰、伤痕或状态变化；与“{name}”保持可辨认的同一角色核心特征，但绝不沿用其他形态的年龄和服装；完整单人角色设定图，无文字水印。"
        ));
    }
}

fn fallback_description(kind: &str, name: &str, index: usize) -> String {
    match kind {
        "character" => format!("{name}是推动故事发展的核心人物，待人方式克制而真诚，遇到压力会先观察细节再作出决定，并把对重要之人的承诺落实为行动。"),
        "scene" => format!("{name}是角色交换信息、推进冲突或揭开线索的可复用剧情空间。"),
        _ => format!("{name}是连接人物关系或推进下一步行动的关键叙事物件，第{}次出现时仍可被清晰识别。", index + 1),
    }
}

fn character_appearance(index: usize, female: bool) -> &'static str {
    const FEMALE: [&str; 3] = [
        "年龄与生命阶段必须以剧本指定的基础形态为准，鹅蛋脸，肤色自然，眉眼清晰，长发或利落短发，身形比例符合该阶段；服装采用与身份相符的耐穿面料，保留一件可辨认的随身饰物。",
        "轮廓利落，眼神坚定，发型整洁，体态轻盈有力量；不得擅自成年化或幼态化，服装的颜色、剪裁、鞋履和配饰共同体现当前基础阶段的行动习惯。",
        "脸部线条柔和但不失警觉，发丝和妆容克制，身形比例符合剧本阶段；衣料有真实褶皱与使用痕迹，配饰简洁并具有记忆点。",
    ];
    const MALE: [&str; 3] = [
        "年龄与生命阶段必须以剧本指定的基础形态为准，轮廓清晰，短发利落，眉眼有警觉感，身形比例符合该阶段；服装采用与身份相符的耐穿面料，保留一件可辨认的随身饰物。",
        "脸部棱角与目光状态遵从基础形态，发型整洁，体态体现行动习惯；不得擅自成年化或幼态化，服装的颜色、剪裁、鞋履和配饰共同体现当前阶段。",
        "眉骨、皮肤状态与身形比例符合剧本阶段，发丝自然；衣料有真实褶皱与使用痕迹，配饰简洁并具有记忆点。",
    ];
    if female {
        FEMALE[index % FEMALE.len()]
    } else {
        MALE[index % MALE.len()]
    }
}

fn scene_structure(index: usize) -> &'static str {
    [
        "前景遮挡、中景行动区和远景环境形成明确纵深，主体建筑或自然环境具有可识别轮廓。",
        "入口、通道和核心区域的动线清楚，墙面、地面与主要陈设保持同一时代和使用尺度。",
        "空间保留一处可供人物停留或对峙的视觉中心，前后景层次清晰，方便持续作为分镜参考。",
    ][index % 3]
}

fn prop_details(index: usize) -> &'static str {
    [
        "颜色克制，尺寸符合手持或陈列用途；金属、木材、纸张或织物等材质边缘有细微磨损，表面装饰与故事背景一致。",
        "主体的形制和比例清楚，纹理、接缝、刻痕或封口等关键细节可近看识别，保留长期使用形成的自然痕迹。",
        "材质反光与阴影真实，边角、挂件或局部纹样有可辨特征；如需文字，仅保留剧情必要且不可读的简短符号。",
    ][index % 3]
}

fn matching_voice(name: &str, source: &str) -> &'static str {
    let text = format!("{name}{source}");
    if is_female(name, source) {
        if contains(&text, &["冷", "刑警", "克制", "御姐"]) {
            "cold_royal_sister_female"
        } else if contains(&text, &["坚", "强", "行动", "果断"]) {
            "strong_female_lead"
        } else if contains(&text, &["成熟", "母", "温柔", "照顾"]) {
            "mature_warm_goddess_female"
        } else {
            "sweet_fox_tease_female"
        }
    } else if contains(&text, &["反派", "阴郁", "病娇", "疯狂"]) {
        "sickly_gloomy_yandere_male"
    } else if contains(&text, &["师父", "长者", "老人", "父亲", "首领"]) {
        "ruthless_old_fox_male"
    } else if contains(&text, &["少年", "青年", "学生", "新人"]) {
        "cool_career_newcomer_male"
    } else {
        "warm_older_brother_male"
    }
}

fn is_female(name: &str, source: &str) -> bool {
    contains(
        &format!("{name}{source}"),
        &["女性", "女孩", "少女", "女主", "妻", "母", "姐", "妹", "她"],
    )
}

fn contains(value: &str, terms: &[&str]) -> bool {
    terms.iter().any(|term| value.contains(term))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::enrich;

    #[test]
    fn generated_assets_keep_private_prompts_and_character_voice_ids() {
        let mut plan = json!({"assets":[
            {"type":"character","name":"苏晚","prompt":"青年女性调查者，面对危险仍会追查真相。"},
            {"type":"scene","name":"旧站房","prompt":"角色发现线索的旧车站。"},
            {"type":"prop","name":"泛黄信件","prompt":"揭示失踪线索的信件。"}
        ]});

        enrich(&mut plan, "都市", "真人风格");
        let assets = plan["assets"].as_array().expect("assets");
        assert!(assets.iter().all(|asset| asset["prompt"]
            .as_str()
            .is_some_and(|text| text.starts_with("叙述背景主题：都市\n风格：真人风格\n"))));
        assert!(assets[0]["voice_id"]
            .as_str()
            .is_some_and(|id| id != "none"));
        assert!(assets[0]["prompt"]
            .as_str()
            .is_some_and(|text| text.contains("外观设定")));
        assert!(assets[1]["prompt"]
            .as_str()
            .is_some_and(|text| text.contains("空间与主体")));
        assert!(assets[2]["prompt"]
            .as_str()
            .is_some_and(|text| text.contains("外观细节")));
    }

    #[test]
    fn extracted_child_form_keeps_its_own_age_and_wardrobe_prompt() {
        let mut plan = json!({"assets":[{
            "type":"character","name":"林砚","prompt":"成年后成为克制的剑修。",
            "variants":[{"name":"幼年形态","prompt":"八岁，圆脸，短发，粗布短褂，赤脚。"}]
        }]});
        enrich(&mut plan, "仙侠", "2D动漫风");
        let form = &plan["assets"][0]["variants"][0]["prompt"];
        assert!(form.as_str().is_some_and(|text| text.contains("八岁")
            && text.contains("幼年形态")
            && text.contains("风格：2D动漫风")));
    }

    #[test]
    fn generated_prompt_does_not_repeat_model_theme_or_style_headers() {
        let mut plan = json!({"assets":[{
            "type":"character","name":"林岩",
            "prompt":"叙述背景主题：玄幻\n风格：真人风格\n青年剑修，遇险时先观察环境再保护同伴。"
        }]});

        enrich(&mut plan, "玄幻", "真人风格");

        assert_eq!(
            plan["assets"][0]["prompt"]
                .as_str()
                .expect("prompt")
                .matches("叙述背景主题：")
                .count(),
            1,
        );
        assert!(plan["assets"][0]["prompt"]
            .as_str()
            .is_some_and(|text| text.contains("角色身份与性格：青年剑修")));
    }
}
