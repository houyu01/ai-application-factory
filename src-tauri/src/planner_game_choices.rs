//! Quality rules for player-facing decisions in an interactive-video graph.

#[cfg(test)]
pub(crate) const CHOICE_LABEL_CONTRACT: &str = "选择文案质量（最高优先级）：option_text 是玩家在来源视频结束时点击的、会直接过渡到目标视频的一句话抉择，不是节点标题、线索编号、路线名称或后果说明。它必须写成具体可执行的动作、对话回答、态度立场或信息处置，且同一来源的各项必须互斥、有明显不同的即时行动或关系后果。可以短，如“举手”“不举手”“答应”“推脱”；也可以是有角色口吻的完整回答，如“没问题，请领事放心！”“日军滔天罪行，中国人岂会忘记”“这是我应该的，钱坚决不要！”。也可写策略或处置，如“和平谈判，通过外交解决争端”“交份真实而完整的名单”“委婉拒绝，这是别人的心意”。若扩写稿里的选择仍是“线索1”等占位标签，保留它原有的前往 ID、状态和后果，但必须依据来源与目标视频改写 option_text，绝不可照抄占位标签。绝不可输出“线索1/线索2”“选项A”“路径1”“路线2”“继续”“下一步”“调查路径”等编号或公式化文案；不要把“选择后的结果”写成选项。";

/// Require choice edges to be the visible, causal bridge between independently generated videos.
pub(crate) const VIDEO_NODE_TRANSITION_CONTRACT: &str = "视频节点剧情衔接（最高优先级，生成前后都必须逐边自检）：每一条选择边都是两个视频节点之间不可省略的剧情桥，必须满足“来源节点 original_text → option_text → 目标节点 original_text”的完整因果链。来源节点的 original_text 要可拍摄地写清导致抉择的最后动作、信息、问题或人物状态，并让视频结束时停在玩家尚未选择的节点；option_text 是唯一把两个视频串起来的玩家可见文案，必须精确承接来源原文末态，写成玩家实际执行的动作、回答、立场或信息处置；目标节点的 original_text 必须把该选择的执行及立即后果写成开场或核心剧情，禁止无因跳场、另起事件或与选择矛盾。目标原文可以自然复述 option_text，但选择的动作、信息或关系后果必须能在 original_text 中明确定位，不能只写在 edge 里。多条入边汇合时，目标 original_text 只能呈现各条选择均可导向的共同后果；各入边仍须分别通过自身 option_text 解释如何到达该开场，路径差异保留在 edge.conditions。绝不可只用节点标题或泛化提示词连接边。每个节点 prompt 的“前序承接”必须说明入边选择如何导入本节点原文，“选择后果”必须说明本节点原文结尾将由哪些出边选择继续推进；生成视频时以 original_text 中这条因果链为准，不在视频内替玩家提前做出选择。";

const GENERIC_LABELS: &[&str] = &[
    "选项",
    "选择",
    "继续",
    "继续推进",
    "继续调查",
    "继续探索",
    "下一步",
    "前往",
    "调查",
    "路线",
    "路径",
    "线索",
    "待定",
];
const NUMBERED_PREFIXES: &[&str] = &[
    "线索",
    "关键线索",
    "选项",
    "选择",
    "路径",
    "路线",
    "调查路径",
];

pub(crate) fn is_meaningful_choice_label(value: &str) -> bool {
    let compact = choice_label_key(value);
    !compact.is_empty()
        && !GENERIC_LABELS.contains(&compact.as_str())
        && !numbered_placeholder(&compact)
}

pub(super) fn fallback_choice_label(option: &str, target_title: &str) -> String {
    let option = option.trim();
    if is_meaningful_choice_label(option) {
        return option.to_owned();
    }
    format!("前往「{target_title}」")
}

pub(crate) fn choice_label_key(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            !character.is_whitespace()
                && !matches!(character, '，' | ',' | '。' | '！' | '!' | '？' | '?')
        })
        .flat_map(char::to_lowercase)
        .collect()
}

fn numbered_placeholder(value: &str) -> bool {
    NUMBERED_PREFIXES
        .iter()
        .any(|prefix| value.strip_prefix(prefix).is_some_and(ordinal_suffix))
}

fn ordinal_suffix(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_digit() || "一二三四五六七八九十甲乙丙丁abcd".contains(character)
        })
}

#[cfg(test)]
mod tests {
    use super::is_meaningful_choice_label;

    #[test]
    fn accepts_player_facing_actions_answers_and_stances() {
        for label in [
            "举手",
            "不举手",
            "和平谈判，通过外交解决争端",
            "没问题，请领事放心！",
            "交份真实而完整的名单",
            "日军滔天罪行，中国人岂会忘记",
            "这是我应该的，钱坚决不要！",
            "委婉拒绝，这是别人的心意",
        ] {
            assert!(is_meaningful_choice_label(label), "rejected {label}");
        }
    }

    #[test]
    fn rejects_numbered_or_formulaic_choice_labels() {
        for label in [
            "线索1",
            "线索二",
            "选项 A",
            "路径3",
            "调查路径甲",
            "继续",
            "下一步",
        ] {
            assert!(!is_meaningful_choice_label(label), "accepted {label}");
        }
    }
}
