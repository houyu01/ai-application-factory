//! Parse the structured branch screenplay into a typed IR for deterministic graph compilation.

use serde_json::Value;

#[derive(Clone, Debug)]
pub(super) struct ScreenplayBeat {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub scene: String,
    pub cast: String,
}

#[derive(Clone, Debug)]
pub(super) struct ScreenplayChoice {
    pub source: String,
    pub option: String,
    pub target: String,
    pub requires: Option<(String, Value)>,
    pub set: Option<(String, Value)>,
}

#[derive(Clone, Debug)]
pub(super) struct ScreenplayIr {
    pub beats: Vec<ScreenplayBeat>,
    pub choices: Vec<ScreenplayChoice>,
}

/// Read Sxx / Exx blocks and their 前往 links. Missing choices are allowed; topology repair happens later.
pub(super) fn parse_game_screenplay(text: &str) -> Option<ScreenplayIr> {
    let mut beats = Vec::new();
    let mut choices = Vec::new();
    let mut current: Option<OpenBlock> = None;
    let mut choice_source: Option<String> = None;
    let mut pending: Option<OpenChoice> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(header) = parse_segment_header(line) {
            flush_choice(&mut choices, &mut pending);
            flush_block(&mut beats, &mut current);
            choice_source = Some(header.id.clone());
            current = Some(OpenBlock {
                beat: header,
                lines: Vec::new(),
            });
            continue;
        }
        if let Some(header) = parse_ending_header(line) {
            flush_choice(&mut choices, &mut pending);
            flush_block(&mut beats, &mut current);
            choice_source = None;
            current = Some(OpenBlock {
                beat: header,
                lines: Vec::new(),
            });
            continue;
        }
        if line.contains("【玩家抉择】") {
            flush_choice(&mut choices, &mut pending);
            if let Some(block) = current.as_ref() {
                choice_source = Some(block.beat.id.clone());
            }
            continue;
        }
        if let Some(option) = parse_choice_label(line) {
            flush_choice(&mut choices, &mut pending);
            if let Some(source) = choice_source.clone() {
                pending = Some(OpenChoice {
                    source,
                    option,
                    target: String::new(),
                    requires: None,
                    set: None,
                });
            }
            continue;
        }
        if let Some(choice) = pending.as_mut() {
            if let Some(target) = parse_goto(line) {
                choice.target = target;
                continue;
            }
            if let Some(value) = parse_labeled(line, "触发条件") {
                choice.requires = parse_state_pair(&value);
                continue;
            }
            if let Some(value) = parse_labeled(line, "状态变化") {
                choice.set = parse_state_pair(&value);
                continue;
            }
        }
        if let Some(block) = current.as_mut() {
            block.lines.push(line.to_owned());
        }
    }
    flush_choice(&mut choices, &mut pending);
    flush_block(&mut beats, &mut current);
    if beats.is_empty() {
        return None;
    }
    Some(ScreenplayIr { beats, choices })
}

struct OpenBlock {
    beat: ScreenplayBeat,
    lines: Vec<String>,
}

struct OpenChoice {
    source: String,
    option: String,
    target: String,
    requires: Option<(String, Value)>,
    set: Option<(String, Value)>,
}

fn parse_segment_header(line: &str) -> Option<ScreenplayBeat> {
    let inner = bracket_inner(line)?;
    let rest = inner.strip_prefix("剧情段")?.trim();
    let (id, title) = split_id_title(rest, 'S')?;
    let kind = if id == "s01" || title.contains("开始") {
        "start"
    } else {
        "normal"
    };
    Some(ScreenplayBeat {
        id,
        kind: kind.to_owned(),
        title: non_empty_title(title, "剧情段"),
        body: String::new(),
        scene: String::new(),
        cast: String::new(),
    })
}

fn parse_ending_header(line: &str) -> Option<ScreenplayBeat> {
    let inner = bracket_inner(line)?;
    let rest = inner.strip_prefix("结局")?.trim();
    let (id, title) = split_id_title(rest, 'E')?;
    let kind = if title.contains("成功") {
        "success"
    } else {
        "failure"
    };
    Some(ScreenplayBeat {
        id,
        kind: kind.to_owned(),
        title: non_empty_title(
            title,
            if kind == "success" {
                "成功结局"
            } else {
                "失败结局"
            },
        ),
        body: String::new(),
        scene: String::new(),
        cast: String::new(),
    })
}

fn bracket_inner(line: &str) -> Option<&str> {
    line.trim().strip_prefix('【')?.strip_suffix('】')
}

fn split_id_title(rest: &str, expected: char) -> Option<(String, String)> {
    let rest = rest.trim();
    let first = rest.chars().next()?;
    if !first.eq_ignore_ascii_case(&expected) {
        return None;
    }
    let digits: String = rest
        .chars()
        .skip(1)
        .take_while(|character| character.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    let consumed = 1 + digits.len();
    let number: u32 = digits.parse().ok()?;
    let id = format!("{}{number:02}", expected.to_ascii_lowercase());
    let title = rest
        .get(consumed..)
        .unwrap_or_default()
        .trim()
        .trim_start_matches(['｜', '|', ':', '：'])
        .trim()
        .to_owned();
    Some((id, title))
}

fn parse_choice_label(line: &str) -> Option<String> {
    let line = line.trim().trim_start_matches('-').trim();
    parse_labeled(line, "选择").filter(|value| !value.is_empty())
}

fn parse_goto(line: &str) -> Option<String> {
    let value = parse_labeled(line, "前往")?;
    let token = value
        .split(['；', ';', '，', ',', '。', ' ', '：', ':'])
        .find_map(parse_target_id)?;
    Some(token)
}

fn parse_target_id(token: &str) -> Option<String> {
    let token = token.trim();
    if token.eq_ignore_ascii_case("sxx") || token.eq_ignore_ascii_case("exx") {
        return None;
    }
    split_id_title(token, 'S')
        .or_else(|| split_id_title(token, 'E'))
        .map(|(id, _)| id)
}

fn parse_labeled(line: &str, label: &str) -> Option<String> {
    let line = line.trim().trim_start_matches('-').trim();
    let rest = line.strip_prefix(label)?.trim();
    let rest = rest.trim_start_matches(['：', ':']).trim();
    Some(rest.to_owned())
}

fn parse_state_pair(value: &str) -> Option<(String, Value)> {
    let value = value.trim();
    if value.is_empty() || value == "无" {
        return None;
    }
    let (key, raw) = value.split_once('=')?;
    let key = key.trim();
    if key.is_empty()
        || key.len() > 64
        || !key.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
    {
        return None;
    }
    let raw = raw.trim();
    let parsed = if raw == "true" {
        Value::Bool(true)
    } else if raw == "false" {
        Value::Bool(false)
    } else if let Ok(number) = raw.parse::<i64>() {
        json_number(number)
    } else {
        Value::String(raw.to_owned())
    };
    Some((key.to_owned(), parsed))
}

fn json_number(value: i64) -> Value {
    Value::Number(value.into())
}

fn flush_block(beats: &mut Vec<ScreenplayBeat>, current: &mut Option<OpenBlock>) {
    let Some(mut block) = current.take() else {
        return;
    };
    let scene = field_or_rest(&block.lines, "场景");
    let cast = field_or_rest(&block.lines, "出场角色与道具");
    let body = field_or_rest(&block.lines, "剧情正文")
        .or_else(|| field_or_rest(&block.lines, "结局正文"))
        .unwrap_or_else(|| leftover_body(&block.lines));
    if beats.iter().any(|beat| beat.id == block.beat.id) {
        return;
    }
    if beats.is_empty() && block.beat.kind == "normal" {
        block.beat.kind = "start".to_owned();
    }
    block.beat.scene = scene.unwrap_or_default();
    block.beat.cast = cast.unwrap_or_default();
    block.beat.body = body;
    beats.push(block.beat);
}

fn flush_choice(choices: &mut Vec<ScreenplayChoice>, pending: &mut Option<OpenChoice>) {
    let Some(choice) = pending.take() else {
        return;
    };
    if choice.target.is_empty() || choice.source == choice.target {
        return;
    }
    choices.push(ScreenplayChoice {
        source: choice.source,
        option: choice.option,
        target: choice.target,
        requires: choice.requires,
        set: choice.set,
    });
}

fn field_or_rest(lines: &[String], label: &str) -> Option<String> {
    let mut collecting = false;
    let mut body = String::new();
    for line in lines {
        if let Some(value) = parse_labeled(line, label) {
            collecting = true;
            body = value;
            continue;
        }
        if collecting {
            if parse_labeled(line, "场景").is_some()
                || parse_labeled(line, "出场角色与道具").is_some()
                || parse_labeled(line, "剧情正文").is_some()
                || parse_labeled(line, "结局正文").is_some()
                || parse_labeled(line, "达成条件").is_some()
            {
                break;
            }
            if !line.is_empty() {
                if !body.is_empty() {
                    body.push('\n');
                }
                body.push_str(line);
            }
        }
    }
    let body = body.trim();
    (!body.is_empty()).then(|| body.to_owned())
}

fn leftover_body(lines: &[String]) -> String {
    lines
        .iter()
        .filter(|line| {
            parse_labeled(line, "场景").is_none()
                && parse_labeled(line, "出场角色与道具").is_none()
                && parse_labeled(line, "达成条件").is_none()
                && !line.is_empty()
        })
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

fn non_empty_title(title: String, fallback: &str) -> String {
    if title.trim().is_empty() {
        fallback.to_owned()
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::parse_game_screenplay;

    #[test]
    fn parses_segment_choices_and_endings() {
        let ir = parse_game_screenplay(
            "【剧情段 S01｜开始】\n场景：钟楼入口\n剧情正文：警报响起。\n【玩家抉择】\n- 选择：循着录音上楼\n  触发条件：无\n  状态变化：token=true\n  前往：S02；进入旋梯\n- 选择：砸开木门\n  前往：E02\n【剧情段 S02｜旋梯】\n剧情正文：调查员沿旋梯前进。\n【玩家抉择】\n- 选择：敲响钟绳\n  前往：E01\n【结局 E01｜成功】\n结局正文：真相公开。\n【结局 E02｜失败】\n结局正文：闸门落下。",
        )
        .expect("parsed");

        assert_eq!(ir.beats[0].id, "s01");
        assert_eq!(ir.beats[0].kind, "start");
        assert_eq!(ir.beats[1].id, "s02");
        assert_eq!(ir.beats[2].kind, "success");
        assert_eq!(ir.beats[3].kind, "failure");
        assert_eq!(ir.choices.len(), 3);
        assert_eq!(ir.choices[0].target, "s02");
        assert_eq!(ir.choices[0].set.as_ref().unwrap().0, "token");
    }
}
