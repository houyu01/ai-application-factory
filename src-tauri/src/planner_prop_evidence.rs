//! Conservative noun extraction for explicit prop lists and unambiguous screenplay actions.

use std::collections::HashSet;

const PROP_TERMS: &[&str] = &[
    "西瓜种植手册",
    "农科站检测报告",
    "蜜瓜样品",
    "牛皮纸袋",
    "铜烟袋",
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
    "竹筐",
    "石墩",
    "名片",
    "茶缸",
    "吊扇",
    "蜜瓜",
    "西瓜",
    "剑",
];
const PROP_ACTIONS: &[&str] = &[
    "拿", "拎", "举", "抱", "握", "递", "找", "寻", "掏", "打开", "关上", "放下", "收起", "使用",
    "展示", "摊开", "塞进", "拔出", "指着", "啃", "吃", "喝", "戴上", "摘下", "挂起", "摆着",
    "放着", "放",
];
const PROP_NOUN_SUFFIXES: &[&str] = &[
    "报告",
    "手册",
    "样品",
    "烟袋",
    "纸袋",
    "录音机",
    "身份证",
    "遥控器",
    "扳指",
    "玉佩",
    "令牌",
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
    "竹筐",
    "石墩",
    "名片",
    "茶缸",
    "吊扇",
    "瓜",
    "剑",
    "刀",
    "枪",
    "袋",
    "筐",
    "盒",
    "箱",
    "瓶",
    "杯",
    "碗",
    "盘",
    "锁",
    "表",
    "镜",
    "书",
    "册",
    "卡",
    "牌",
    "印",
    "笔",
    "扇",
    "灯",
    "钟",
    "屏",
    "机",
    "琴",
    "棍",
    "杖",
    "鞭",
    "旗",
    "伞",
    "绳",
    "链",
];
const CONTEXT_PREFIXES: &[&str] = &[
    "拿着", "拎着", "抱着", "握着", "举着", "带着", "背着", "掏出", "递出", "展示", "打开", "找到",
    "捡起", "放下", "收起", "塞进", "挂着", "戴着",
];

pub(crate) fn prop_names(script: &str, characters: &[String]) -> Vec<String> {
    let mut names = explicit_prop_names(script, characters);
    names.extend(PROP_TERMS.iter().filter_map(|term| {
        script
            .match_indices(term)
            .any(|(offset, _)| prop_is_used(script, offset, term.len()))
            .then(|| (*term).to_owned())
    }));
    specific_names(names)
}

fn explicit_prop_names(script: &str, characters: &[String]) -> Vec<String> {
    ["道具：", "道具:", "物品：", "物品:"]
        .into_iter()
        .flat_map(|marker| script.match_indices(marker))
        .flat_map(|(offset, marker)| {
            script[offset + marker.len()..]
                .split(['\n', '。', '；', ';'])
                .next()
                .unwrap_or_default()
                .split(['、', '，', ',', '/', '／'])
        })
        .filter_map(|item| explicit_prop_name(item, characters))
        .collect()
}

fn explicit_prop_name(item: &str, characters: &[String]) -> Option<String> {
    let mut name = item.trim().trim_matches(|character: char| {
        matches!(character, '“' | '”' | '"' | '【' | '】' | '[' | ']')
    });
    if let Some((_, tail)) = name.rsplit_once('的') {
        name = tail.trim();
    }
    if let Some(tail) = CONTEXT_PREFIXES
        .iter()
        .filter_map(|prefix| name.rsplit_once(prefix).map(|(_, tail)| tail.trim()))
        .max_by_key(|tail| tail.chars().count())
    {
        name = tail;
    }
    let name = name.trim();
    let is_character = characters.iter().any(|character| character == name);
    (is_prop_noun(name)
        && !is_character
        && !PROP_ACTIONS.iter().any(|action| name.contains(action)))
    .then(|| name.to_owned())
}

fn is_prop_noun(name: &str) -> bool {
    let count = name.chars().count();
    (1..=16).contains(&count)
        && ![
            "我", "你", "他", "她", "它", "我们", "你们", "他们", "她们", "道具", "物品",
        ]
        .contains(&name)
        && PROP_NOUN_SUFFIXES
            .iter()
            .any(|suffix| name.ends_with(suffix))
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

fn specific_names(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let values = values
        .into_iter()
        .filter(|value| seen.insert(value.to_owned()))
        .collect::<Vec<_>>();
    values
        .iter()
        .enumerate()
        .filter(|(index, name)| {
            !values.iter().enumerate().any(|(other_index, other)| {
                index != &other_index
                    && other.contains(name.as_str())
                    && !is_distinct_compound_prop(name, other)
            })
        })
        .map(|(_, name)| name.to_owned())
        .collect()
}

fn is_distinct_compound_prop(name: &str, other: &str) -> bool {
    !name.is_empty()
        && ["样品", "手册", "报告"]
            .iter()
            .any(|suffix| other.ends_with(suffix))
}
