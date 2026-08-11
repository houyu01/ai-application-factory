//! Branch-screenplay instructions and an offline-readable fallback for interactive games.

use serde_json::Value;

use super::integer;

/// Build the branch-screenplay contract consumed by the later game-graph planner.
pub(crate) fn game_expansion_prompt(game: &Value) -> String {
    let minimum = integer(game, "expanded_script_min_chars", 5_000, 1, 1_000_000);
    let maximum = integer(
        game,
        "expanded_script_max_chars",
        10_000,
        minimum,
        1_000_000,
    );
    let existing = game["expanded_script"].as_str().unwrap_or_default().trim();
    let continuation = !existing.is_empty();
    let structured_existing = existing.contains("【剧情段 S") && existing.contains("【玩家抉择】");
    let success = integer(game, "success_ending_count", 2, 1, 100);
    let failure = integer(game, "failure_ending_count", 30, 1, 200);
    format!(
        "你是互动视频游戏编剧。{mode}。输出可供后续图谱拆分的分支剧本，不要把它写成连续小说段落；不要输出 JSON、Markdown 代码块或创作说明。\n\n完整剧本长度目标为 {minimum} 至 {maximum} 个中文字符。每个剧情段都必须有具体、可拍摄的场景、动作、对白与信息变化，并只承载一个可单独播放的视频节点或一段可继续细拆的连续行动。保持角色、场景、道具、时间线和视觉风格连续；视觉风格为“{style}”，节点视频分辨率为“{resolution}”。\n\n严格使用下列纯文本结构，标题、字段名和 ID 都不可省略：\n【互动剧本总览】\n世界与冲突：…\n玩家目标：…\n\n【状态变量】\n- state_key：含义；由哪次选择写入；会在何处影响选项或结局。\n\n【剧情段 S01｜开始】\n场景：…\n出场角色与道具：…\n剧情正文：…\n\n【玩家抉择】\n- 选择：玩家能看见且能理解的动作或回答。\n  触发条件：无，或需要的 state_key=value。\n  状态变化：写入的 state_key=value；没有则写“无”。\n  前往：Sxx 或 Exx；说明这项选择造成的即时后果。\n\n每一个后续剧情段继续使用“【剧情段 Sxx｜标题】”和“【玩家抉择】”。线性承接也要写一个唯一的“继续推进”选择及其前往 ID。不同路线可以汇合到同一个 Sxx；汇合后必须保留先前状态，使后续选择或结局能因状态不同而出现不同后果。不要使用“选项 A”“继续”“路线 1”这类空泛文案。\n\n最后逐一列出所有终局：\n【结局 E01｜成功】或【结局 E01｜失败】\n达成条件：需要的状态、最后一次选择与因果关系。\n结局正文：可拍摄的收束场景、角色结果与情绪。\n\n必须恰好设计 {success} 个成功结局和 {failure} 个失败结局。失败应分散在不同抉择深度，允许早期错误直接失败；每个结局的达成条件和画面结果必须不同。至少设计一组“早期状态写入、跨分支汇合、后期状态读取并兑现”的影响，状态键必须为简短 snake_case，值只能是字符串、数字或布尔值。\n\n原始剧本：\n{source}\n\n已保存的分支剧本：\n{existing}",
        mode = if !continuation {
            "请从原始创意开始设计完整的分支剧本，不要跳过抉择的前因、条件和后果。"
        } else if structured_existing {
            "请只追加新的“【剧情段 Sxx｜…】”或“【结局 Exx｜…】”区块以扩展尚未展开的路线；引用已有 ID，不复述已有区块，也不要重新编号。"
        } else {
            "已保存内容是旧版单线正文。请根据它输出一份完整、自包含的分支剧本，从“【互动剧本总览】”开始并使用 S01/E01 编号；系统会保留旧正文，但后续图谱会以这份结构化剧本为准。"
        },
        style = game["style"].as_str().unwrap_or("真人风格"),
        resolution = game["resolution"].as_str().unwrap_or("720p"),
        source = game["script"].as_str().unwrap_or_default(),
    )
}

/// Keep offline creation reviewable by retaining the same branch-screenplay shape as model output.
pub(crate) fn fallback_game_expansion(game: &Value) -> String {
    let source = game["script"].as_str().unwrap_or_default().trim();
    let success = integer(game, "success_ending_count", 2, 1, 100);
    let failure = integer(game, "failure_ending_count", 30, 1, 200);
    let mut screenplay = format!(
        "【互动剧本总览】\n世界与冲突：{source}\n玩家目标：在风险升级前辨别关键信息，做出能改变结果的选择。\n\n【状态变量】\n- evidence_secured：是否取得可验证的关键证据；在 S02 的调查选择写入；在终局决定能否安全揭示真相。\n\n【剧情段 S01｜危机开始】\n场景：原始剧本中的首个关键地点。\n出场角色与道具：主角、推动冲突的关键人物或物件。\n剧情正文：{source}\n\n【玩家抉择】\n- 选择：谨慎观察现场并确认线索。\n  触发条件：无。\n  状态变化：无。\n  前往：S02；获得继续调查的机会。\n- 选择：在信息不足时贸然行动。\n  触发条件：无。\n  状态变化：无。\n  前往：E{:02}；立即暴露于风险。\n\n【剧情段 S02｜线索取舍】\n场景：与首个地点连贯的调查区域。\n出场角色与道具：主角、关键线索与阻碍者。\n剧情正文：主角发现两条互相矛盾的线索，必须在时间压力下决定是否保全证据。\n\n【玩家抉择】\n- 选择：带走可验证的证据，再前往真相所在处。\n  触发条件：无。\n  状态变化：evidence_secured=true。\n  前往：S03；证据被妥善保留。\n- 选择：相信未经验证的说辞并丢下证据。\n  触发条件：无。\n  状态变化：evidence_secured=false。\n  前往：S03；表面上节省时间，却失去关键筹码。\n\n【剧情段 S03｜真相对峙】\n场景：冲突最终发生的封闭空间。\n出场角色与道具：主角、对手、关键证据。\n剧情正文：主角与对手正面相遇，先前保留的证据决定其是否能证明真相并脱身。\n\n【玩家抉择】\n- 选择：在证据完整时公开真相并请求支援。\n  触发条件：evidence_secured=true。\n  状态变化：无。\n  前往：E01；让早期调查的成果兑现。\n- 选择：没有证据仍孤身指控对手。\n  触发条件：evidence_secured=false。\n  状态变化：无。\n  前往：E{:02}；对手反转局势。\n\n",
        success + 1,
        success + failure,
    );
    for ordinal in 1..=success {
        screenplay.push_str(&format!(
            "【结局 E{ordinal:02}｜成功】\n达成条件：保留关键证据，并在最终对峙时做出能让真相被验证的选择。\n结局正文：主角以不同方式化解危机，真相得到确认，关系与世界状态出现积极但各不相同的收束。\n\n"
        ));
    }
    for ordinal in 1..=failure {
        let ending = success + ordinal;
        screenplay.push_str(&format!(
            "【结局 E{ending:02}｜失败】\n达成条件：在第 {ordinal} 个风险点忽略信息、失去证据或做出不可逆的错误选择。\n结局正文：危机以第 {ordinal} 种不同方式恶化，主角失去推进目标、同伴或脱身机会，留下与该错误相对应的可拍摄后果。\n\n"
        ));
    }
    screenplay.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{fallback_game_expansion, game_expansion_prompt};

    #[test]
    fn expansion_prompt_requires_a_conditioned_branch_screenplay() {
        let game = json!({
            "script":"西瓜村的陌生人带来一颗蜜瓜。",
            "success_ending_count":2,
            "failure_ending_count":3,
            "expanded_script_min_chars":100,
            "expanded_script_max_chars":200,
        });
        let prompt = game_expansion_prompt(&game);

        for required in [
            "【剧情段 S01｜开始】",
            "【玩家抉择】",
            "触发条件：",
            "状态变化：",
            "前往：Sxx 或 Exx",
            "2 个成功结局和 3 个失败结局",
            "snake_case",
        ] {
            assert!(prompt.contains(required), "missing {required}");
        }
    }

    #[test]
    fn offline_expansion_lists_configured_endings_and_branch_conditions() {
        let screenplay = fallback_game_expansion(&json!({
            "script":"玩家在钟楼追查失踪搭档。",
            "success_ending_count":2,
            "failure_ending_count":3,
        }));

        assert!(screenplay.contains("【玩家抉择】"));
        assert!(screenplay.contains("触发条件：evidence_secured=true"));
        assert_eq!(screenplay.matches("｜成功】").count(), 2);
        assert_eq!(screenplay.matches("｜失败】").count(), 3);
    }

    #[test]
    fn continuation_converts_legacy_prose_but_appends_to_a_branch_screenplay() {
        let legacy = game_expansion_prompt(&json!({"expanded_script":"旧版连续正文"}));
        let structured = game_expansion_prompt(&json!({
            "expanded_script":"【剧情段 S01｜开始】\n【玩家抉择】"
        }));

        assert!(legacy.contains("旧版单线正文"));
        assert!(structured.contains("只追加新的“【剧情段 Sxx｜…】”"));
    }
}
