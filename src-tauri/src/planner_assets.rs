//! Script-grounded asset fallbacks used only when a model response omits reusable materials.

use std::collections::HashSet;

use serde_json::{json, Value};

use crate::value::NOT_GENERATED;

/// Derive only explicitly mentioned materials so a degraded model response never invents a fixed cast.
pub(crate) fn extracted_assets(script: &str, theme: &str) -> Vec<Value> {
    let mut assets = Vec::new();
    let mut seen = HashSet::new();
    for name in character_names(script) {
        add_asset(
            &mut assets,
            &mut seen,
            "character",
            &name,
            &format!("剧本中出现的人物“{name}”，需要保持身份、行为习惯和外观连续性。"),
            theme,
        );
    }
    for name in scene_names(script) {
        add_asset(
            &mut assets,
            &mut seen,
            "scene",
            &name,
            &format!("剧本中发生实际剧情的场景“{name}”，需要保持空间布局和光线连续性。"),
            theme,
        );
    }
    for name in prop_names(script) {
        add_asset(
            &mut assets,
            &mut seen,
            "prop",
            &name,
            &format!("剧本中出现并参与叙事的道具“{name}”，需要保持材质、形制和使用痕迹连续性。"),
            theme,
        );
    }
    assets
}

fn add_asset(
    assets: &mut Vec<Value>,
    seen: &mut HashSet<String>,
    kind: &str,
    name: &str,
    description: &str,
    theme: &str,
) {
    let name = name.trim();
    if name.is_empty() || !seen.insert(format!("{kind}:{name}")) {
        return;
    }
    assets.push(json!({
        "type": kind,
        "name": name,
        "prompt": format!("叙述背景主题：{theme}\n{description}"),
        "status": NOT_GENERATED,
    }));
}

fn character_names(script: &str) -> Vec<String> {
    let mut names = Vec::new();
    for title in [
        "大师兄",
        "师兄",
        "师姐",
        "师父",
        "师尊",
        "掌门",
        "长老",
        "宗主",
        "少主",
        "公主",
        "王爷",
        "皇帝",
        "皇后",
        "夫人",
        "先生",
        "警官",
        "医生",
        "老板",
    ] {
        for (offset, _) in script.match_indices(title) {
            let name = script[offset + title.len()..]
                .chars()
                .take_while(|character| is_cjk(*character))
                .take(3)
                .collect::<String>();
            if plausible_name(&name) {
                names.push(name);
            }
        }
    }
    for action in [
        "穿", "抱", "靠", "笑", "握", "走", "看", "说", "道", "抬", "挥", "坐", "站", "追", "跑",
        "拿", "推", "听", "望", "跪", "喝", "喊", "哭", "问", "答",
    ] {
        for (offset, _) in script.match_indices(action) {
            let name = cjk_tail(&script[..offset], 2);
            if plausible_name(&name) {
                names.push(name);
            }
        }
    }
    unique(names)
}

fn scene_names(script: &str) -> Vec<String> {
    let mut names = script
        .lines()
        .filter_map(|line| {
            line.split_once("场景：")
                .or_else(|| line.split_once("场景:"))
                .map(|(_, value)| clean_scene(value))
        })
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    for suffix in [
        "演武场",
        "练武场",
        "后山",
        "前山",
        "大殿",
        "宫殿",
        "书房",
        "客栈",
        "酒楼",
        "庭院",
        "院落",
        "旧宅",
        "旧居",
        "站房",
        "车站",
        "校园",
        "教室",
        "病房",
        "办公室",
        "工厂",
        "仓库",
        "码头",
        "街道",
        "巷",
        "山洞",
        "山庄",
        "村",
        "府",
        "门派",
    ] {
        for (offset, _) in script.match_indices(suffix) {
            let end = offset + suffix.len();
            let start = scene_start(&script[..offset]);
            let name = clean_scene(&script[start..end]);
            if name.chars().count() >= suffix.chars().count() {
                names.push(name);
            }
        }
    }
    unique(names)
}

fn prop_names(script: &str) -> Vec<String> {
    let mut names = Vec::new();
    for name in [
        "铁刀", "长剑", "短剑", "宝剑", "佩剑", "剑柄", "令牌", "玉佩", "信件", "书信", "钥匙",
        "玉简", "卷轴", "账本", "手机", "电脑", "照片", "药瓶", "手枪", "匕首", "盒子", "包裹",
        "信物", "地图", "酒杯", "木牌", "符箓", "剑",
    ] {
        if script.contains(name) {
            names.push(name.to_owned());
        }
    }
    unique(names)
}

fn clean_scene(value: &str) -> String {
    value
        .split(['\n', '。', '，', '；', '·', '•'])
        .next()
        .unwrap_or_default()
        .trim()
        .trim_matches(|character: char| matches!(character, '日' | '夜' | '内' | '外'))
        .trim()
        .to_owned()
}

fn scene_start(value: &str) -> usize {
    value
        .char_indices()
        .rev()
        .find_map(|(index, character)| {
            matches!(
                character,
                '，' | '。' | '；' | '\n' | '：' | ':' | '在' | '到' | '回' | '入' | '往'
            )
            .then_some(index + character.len_utf8())
        })
        .unwrap_or(0)
}

fn cjk_tail(value: &str, count: usize) -> String {
    value
        .chars()
        .rev()
        .take_while(|character| is_cjk(*character))
        .take(count)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

fn plausible_name(name: &str) -> bool {
    name.chars().count() == 2
        && ![
            "场景", "动作", "镜头", "人物", "角色", "道具", "背景", "一声", "紧握", "大师", "师兄",
            "弟子", "青云", "演武", "廊柱", "铁刀", "青布",
        ]
        .contains(&name)
}

fn unique(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.to_owned()))
        .collect()
}

fn is_cjk(character: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&character)
}

#[cfg(test)]
mod tests {
    use super::extracted_assets;

    #[test]
    fn fallback_catalog_uses_materials_mentioned_in_the_screenplay() {
        let assets = extracted_assets(
            "场景：青云山演武场·日\n动作：二十岁的林砚穿青布道袍练剑，大师兄赵峰抱着铁刀靠在廊柱上，嘲笑一声撞开他的剑。林砚握紧剑柄。",
            "玄幻",
        );
        let names = assets
            .iter()
            .map(|asset| asset["name"].as_str().expect("name"))
            .collect::<Vec<_>>();
        assert!(names.contains(&"林砚"));
        assert!(names.contains(&"赵峰"));
        assert!(names.contains(&"青云山演武场"));
        assert!(names.contains(&"铁刀"));
        assert!(names.contains(&"剑"));
    }
}
