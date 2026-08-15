//! Parsing and structural validation helpers for numbered long-drama screenplay checkpoints.

use serde_json::Value;

use crate::error::{AppError, AppResult};

pub(crate) struct Episode {
    pub(crate) number: i64,
    pub(crate) title: String,
    pub(crate) body: String,
}

pub(crate) trait Fallback {
    fn if_empty(self, fallback: &str) -> String;
}
impl Fallback for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_owned()
        } else {
            self
        }
    }
}

pub(crate) fn target_episode_count(project: &Value) -> AppResult<i64> {
    let value = project["episode_count"].as_i64().unwrap_or(15);
    if (2..=100).contains(&value) {
        Ok(value)
    } else {
        Err(AppError::BadRequest(
            "目标剧集数必须在 2 至 100 集之间".to_owned(),
        ))
    }
}
pub(crate) fn clean(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("```text")
        .trim_start_matches("```markdown")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_owned()
}
pub(crate) fn source_excerpt(value: &str) -> String {
    if char_count(value) <= 12_000 {
        value.to_owned()
    } else {
        format!(
            "{}\n\n……（中间原稿省略）……\n\n{}",
            clip_chars(value, 9_000),
            value
                .chars()
                .rev()
                .take(3_000)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        )
    }
}
pub(crate) fn char_count(value: &str) -> usize {
    value.chars().count()
}
pub(crate) fn expansion_progress(written: usize, target: usize) -> i64 {
    5 + ((55 * written.min(target) + target / 2) / target.max(1)) as i64
}
pub(crate) fn clip_chars(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}
pub(crate) fn join_screenplay(left: &str, right: &str) -> String {
    if left.is_empty() {
        right.to_owned()
    } else {
        format!("{left}\n\n{right}")
    }
}
pub(crate) fn long_ready(value: &str, minimum: usize, target: i64) -> bool {
    char_count(value) >= minimum && episode_sections(value).len() >= target as usize
}
pub(crate) fn resumable(value: &str) -> bool {
    let sections = episode_sections(value);
    !sections.is_empty()
        && sections
            .iter()
            .enumerate()
            .all(|(index, item)| item.number == index as i64 + 1)
}

pub(crate) fn episode_sections(value: &str) -> Vec<Episode> {
    let lines = value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let starts = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| heading(line).map(|(number, title)| (index, number, title)))
        .collect::<Vec<_>>();
    let mut sections = Vec::new();
    let mut used = std::collections::HashSet::new();
    for (index, (line, number, title)) in starts.iter().enumerate() {
        if !used.insert(*number) {
            continue;
        }
        let end = starts
            .get(index + 1)
            .map(|next| next.0)
            .unwrap_or(lines.len());
        let body = lines[line + 1..end].join("\n").trim().to_owned();
        if !body.is_empty() {
            sections.push(Episode {
                number: *number,
                title: title.clone(),
                body,
            });
        }
    }
    sections
}

fn heading(line: &str) -> Option<(i64, String)> {
    let line = line
        .trim()
        .trim_start_matches(['【', '['])
        .trim_end_matches(['】', ']'])
        .trim();
    let after = line.strip_prefix('第')?.trim_start();
    let digits = after
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    let number = digits.parse::<i64>().ok()?;
    let tail = after
        .strip_prefix(&digits)?
        .trim_start()
        .strip_prefix('集')?
        .trim();
    let title = tail.trim_start_matches(['：', ':']).trim();
    Some((
        number,
        if title.is_empty() {
            format!("第{number}集")
        } else {
            title.to_owned()
        },
    ))
}

pub(crate) fn fit_installment(chunk: String, start: i64, end: i64) -> AppResult<String> {
    let sections = episode_sections(&chunk);
    let expected = (start..=end).collect::<Vec<_>>();
    if sections.iter().map(|item| item.number).collect::<Vec<_>>() != expected {
        return Err(AppError::External(format!(
            "扩写剧本集号必须连续覆盖第{start:03}至第{end:03}集"
        )));
    }
    Ok(chunk)
}

pub(crate) fn validate_screenplay(value: &str, target: i64) -> AppResult<()> {
    if episode_sections(value).len() < target as usize {
        return Err(AppError::External(format!(
            "扩写剧本未拆分到 {target} 集，当前为 {} 集",
            episode_sections(value).len()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_episode_count_uses_the_product_default() {
        assert_eq!(target_episode_count(&json!({})).expect("episode count"), 15);
    }
}
