//! Deterministic screenplay evidence catalog that prevents dialogue fragments becoming assets.

use std::collections::HashSet;

const CHARACTER_MARKERS: &[&str] = &["人物首次出现：", "人物首次出现:", "角色：", "角色:"];
const CHARACTER_ACTIONS: &[&str] = &[
    "蹲", "站", "坐", "走", "跑", "冲", "举", "拎", "抱", "拿", "握", "递", "推", "拉", "跪", "挥",
    "拍", "喊", "哭", "笑", "喝", "吻", "扇", "指", "追", "扑", "踹", "开枪", "拔剑", "推门",
    "说道", "问道", "答道",
];
const STABLE_TITLES: &[&str] = &[
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
    "村长",
    "护士",
    "保安",
    "司机",
    "店员",
    "老师",
    "村民甲",
    "村民乙",
    "黑衣人",
    "蒙面人",
];
const COMMON_SURNAMES: &[char] = &[
    '赵', '钱', '孙', '李', '周', '吴', '郑', '王', '冯', '陈', '褚', '卫', '蒋', '沈', '韩', '杨',
    '朱', '秦', '尤', '许', '何', '吕', '施', '张', '孔', '曹', '严', '华', '金', '魏', '陶', '姜',
    '戚', '谢', '邹', '喻', '柏', '水', '窦', '章', '云', '苏', '潘', '葛', '范', '彭', '鲁', '马',
    '苗', '凤', '花', '方', '俞', '任', '袁', '柳', '鲍', '史', '唐', '费', '廉', '岑', '薛', '雷',
    '贺', '倪', '汤', '滕', '罗', '毕', '郝', '邬', '安', '常', '乐', '于', '傅', '皮', '齐', '康',
    '伍', '余', '元', '卜', '顾', '孟', '平', '黄', '和', '穆', '林', '陆', '程', '叶', '白', '莫',
    '江', '宋', '杜', '秦', '萧', '尹', '欧', '诸', '司', '夏',
];
const NON_NAME_CHARS: &[char] = &[
    '的', '地', '得', '着', '了', '后', '前', '中', '里', '外', '上', '下', '间', '时', '天', '夜',
    '者', '园', '朝',
];
const SCENE_LABELS: &[&str] = &["场景", "地点", "场地"];
const SCENE_SUFFIXES: &[&str] = &[
    "演武场",
    "练武场",
    "客厅",
    "书房",
    "办公室",
    "会议室",
    "病房",
    "教室",
    "咖啡馆",
    "餐厅",
    "酒楼",
    "客栈",
    "庭院",
    "院落",
    "旧宅",
    "车站",
    "站房",
    "校园",
    "工厂",
    "仓库",
    "码头",
    "街道",
    "巷口",
    "山洞",
    "山庄",
    "大殿",
    "宫殿",
    "田埂",
    "村口",
    "门派",
    "府内",
    "医院",
    "警局",
    "墓园",
];
const PROP_TERMS: &[&str] = &[
    "牛皮纸袋",
    "录音机",
    "身份证",
    "遥控器",
    "玉扳指",
    "玉佩",
    "令牌",
    "牛皮纸",
    "长剑",
    "短剑",
    "宝剑",
    "佩剑",
    "铁刀",
    "手枪",
    "匕首",
    "药瓶",
    "卷轴",
    "玉简",
    "账本",
    "钥匙",
    "照片",
    "信件",
    "书信",
    "木盒",
    "盒子",
    "包裹",
    "地图",
    "手机",
    "电脑",
    "酒杯",
    "木牌",
    "符箓",
    "剑柄",
    "西瓜",
    "剑",
];
const PROP_ACTIONS: &[&str] = &[
    "拿", "拎", "举", "抱", "握", "递", "找", "寻", "掏", "打开", "关上", "放下", "收起", "使用",
    "展示", "摊开", "塞进", "拔出", "指着", "啃", "吃", "喝", "戴上", "摘下", "挂起", "摆着",
    "放着", "放",
];

/// Canonical, source-backed asset names available to a single decomposition request.
pub(crate) struct AssetEvidence {
    script: String,
    characters: Vec<String>,
    scenes: Vec<String>,
    props: Vec<String>,
}

impl AssetEvidence {
    /// Build a conservative catalog from explicit screenplay labels and unambiguous action context.
    pub(crate) fn from_script(script: &str) -> Self {
        let characters = unique(
            marked_character_names(script)
                .into_iter()
                .chain(dialogue_character_names(script))
                .chain(action_character_names(script))
                .chain(stable_title_names(script))
                .collect(),
        );
        Self {
            script: script.to_owned(),
            characters,
            scenes: unique(scene_names(script)),
            props: specific_names(prop_names(script)),
        }
    }

    pub(crate) fn names(&self, kind: &str) -> Vec<String> {
        match kind {
            "character" => self.characters.clone(),
            "scene" => self.scenes.clone(),
            "prop" => self.props.clone(),
            _ => Vec::new(),
        }
    }

    /// Resolve a model label to an exact screenplay label only when its quoted evidence is verbatim.
    pub(crate) fn canonical_name(
        &self,
        kind: &str,
        supplied_name: &str,
        source_evidence: &str,
    ) -> Option<String> {
        let source_evidence = source_evidence.trim();
        if source_evidence.is_empty()
            || source_evidence.chars().count() > 48
            || !self.script.contains(source_evidence)
        {
            return None;
        }
        self.names(kind)
            .into_iter()
            .filter(|name| source_evidence.contains(name) && supplied_name.contains(name))
            .max_by_key(|name| name.chars().count())
    }
}

fn marked_character_names(script: &str) -> Vec<String> {
    CHARACTER_MARKERS
        .iter()
        .flat_map(|marker| script.match_indices(marker))
        .filter_map(|(offset, marker)| character_label(&script[offset + marker.len()..]))
        .collect()
}

fn dialogue_character_names(script: &str) -> Vec<String> {
    script
        .lines()
        .flat_map(|line| {
            ["：", ":"].into_iter().filter_map(move |divider| {
                line.find(divider)
                    .and_then(|offset| character_label(dialogue_prefix(&line[..offset])))
            })
        })
        .collect()
}

fn dialogue_prefix(value: &str) -> &str {
    value
        .rsplit([
            '\n', '。', '！', '？', '“', '”', '"', '（', '）', '【', '】',
        ])
        .next()
        .unwrap_or(value)
        .trim_matches(|character: char| matches!(character, '[' | ']' | ' ' | '\t'))
}

fn character_label(value: &str) -> Option<String> {
    let name = value
        .trim()
        .chars()
        .take_while(|character| is_cjk(*character) || *character == '甲' || *character == '乙')
        .take(6)
        .collect::<String>();
    plausible_character(&name).then_some(name)
}

fn action_character_names(script: &str) -> Vec<String> {
    CHARACTER_ACTIONS
        .iter()
        .flat_map(|action| script.match_indices(action))
        .filter_map(|(offset, _)| surname_name_from_tail(&cjk_tail(&script[..offset])))
        .collect()
}

fn stable_title_names(script: &str) -> Vec<String> {
    STABLE_TITLES
        .iter()
        .flat_map(|title| {
            script
                .match_indices(title)
                .map(move |(offset, _)| (title, offset))
        })
        .filter_map(|(title, offset)| {
            let after = &script[offset + title.len()..];
            (starts_action(after) && !starts_named_person(after)).then(|| (*title).to_owned())
        })
        .collect()
}

fn scene_names(script: &str) -> Vec<String> {
    let mut names = Vec::new();
    for label in SCENE_LABELS {
        for marker in [format!("{label}："), format!("{label}:")] {
            names.extend(
                script
                    .match_indices(&marker)
                    .filter_map(|(offset, _)| clean_scene(&script[offset + marker.len()..])),
            );
        }
    }
    names.extend(inline_scene_names(script));
    names
}

fn inline_scene_names(script: &str) -> Vec<String> {
    SCENE_SUFFIXES
        .iter()
        .flat_map(|suffix| {
            script
                .match_indices(suffix)
                .map(move |(offset, _)| (offset, suffix))
        })
        .filter_map(|(offset, suffix)| {
            let before = &script[..offset];
            let start = preposition_start(before)?;
            let name = clean_scene(&script[start..offset + suffix.len()])?;
            (name.chars().count() >= suffix.chars().count()).then_some(name)
        })
        .collect()
}

fn preposition_start(value: &str) -> Option<usize> {
    let scope = value
        .rsplit(['\n', '。', '，', '；', '！', '？', '“', '”', '"'])
        .next()
        .unwrap_or(value);
    let scope_offset = value.len() - scope.len();
    scope.char_indices().rev().find_map(|(offset, character)| {
        matches!(character, '在' | '到' | '进' | '回' | '入' | '往' | '从')
            .then_some(scope_offset + offset + character.len_utf8())
    })
}

fn clean_scene(value: &str) -> Option<String> {
    let value = value
        .split(['\n', '。', '，', '；', '！', '？'])
        .next()
        .unwrap_or_default()
        .split(['（', '('])
        .next()
        .unwrap_or_default()
        .trim()
        .trim_matches(|character: char| matches!(character, '【' | '】' | '[' | ']'));
    let words = value
        .split_whitespace()
        .take_while(|word| !is_time_word(word));
    let mut name = words.collect::<Vec<_>>().join(" ");
    for suffix in ["·正午", "·清晨", "·傍晚", "·日", "·夜", "·内", "·外"] {
        name = name.trim_end_matches(suffix).trim().to_owned();
    }
    (!name.is_empty() && name.chars().count() <= 24 && !name.contains('“') && !name.contains('”'))
        .then_some(name)
}

fn is_time_word(word: &str) -> bool {
    [
        "日", "夜", "内", "外", "清晨", "上午", "中午", "正午", "下午", "傍晚", "晴", "雨", "阴",
    ]
    .contains(&word)
}

fn prop_names(script: &str) -> Vec<String> {
    let explicit = ["道具：", "道具:", "物品：", "物品:"]
        .into_iter()
        .flat_map(|marker| script.match_indices(marker))
        .filter_map(|(offset, marker)| {
            script[offset + marker.len()..]
                .split(['\n', '。', '，', '；'])
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        });
    explicit
        .chain(PROP_TERMS.iter().filter_map(|term| {
            script
                .match_indices(term)
                .any(|(offset, _)| prop_is_used(script, offset, term.len()))
                .then(|| (*term).to_owned())
        }))
        .collect()
}

fn prop_is_used(script: &str, offset: usize, length: usize) -> bool {
    let start = script[..offset]
        .char_indices()
        .rev()
        .nth(10)
        .map(|(index, _)| index)
        .unwrap_or(0);
    let end = script[offset + length..]
        .char_indices()
        .nth(10)
        .map(|(index, _)| offset + length + index)
        .unwrap_or(script.len());
    let context = &script[start..end];
    PROP_ACTIONS.iter().any(|action| context.contains(action))
}

fn starts_action(value: &str) -> bool {
    let value = value.trim_start_matches(|character: char| !is_cjk(character));
    CHARACTER_ACTIONS
        .iter()
        .any(|action| value.starts_with(action))
}

fn starts_named_person(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|character| COMMON_SURNAMES.contains(&character))
}

fn surname_name_from_tail(tail: &str) -> Option<String> {
    let characters = tail.chars().collect::<Vec<_>>();
    let start = characters.len().saturating_sub(4);
    (start..characters.len()).find_map(|index| {
        COMMON_SURNAMES
            .contains(&characters[index])
            .then(|| characters[index..].iter().collect::<String>())
            .filter(|name| plausible_character(name))
    })
}

fn cjk_tail(value: &str) -> String {
    value
        .chars()
        .rev()
        .take_while(|character| is_cjk(*character))
        .take(12)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

fn plausible_character(name: &str) -> bool {
    let count = name.chars().count();
    (2..=4).contains(&count)
        && ![
            "场景",
            "动作",
            "镜头",
            "人物",
            "角色",
            "道具",
            "背景",
            "全体村",
            "外来瓜",
            "我知道",
            "清楚地",
        ]
        .contains(&name)
        && !name
            .chars()
            .any(|character| NON_NAME_CHARS.contains(&character))
        && (COMMON_SURNAMES.contains(&name.chars().next().unwrap_or_default())
            || STABLE_TITLES.contains(&name))
}

fn unique(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.to_owned()))
        .collect()
}

fn specific_names(values: Vec<String>) -> Vec<String> {
    let values = unique(values);
    values
        .iter()
        .enumerate()
        .filter(|(index, name)| {
            !values
                .iter()
                .enumerate()
                .any(|(other_index, other)| index != &other_index && other.contains(name.as_str()))
        })
        .map(|(_, name)| name.to_owned())
        .collect()
}

fn is_cjk(character: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&character)
}
