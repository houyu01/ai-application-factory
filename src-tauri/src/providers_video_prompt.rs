//! DashScope-specific conversion of application reference-image markers.

/// Convert the stable project marker syntax into the provider's prompt convention.
pub(super) fn dashscope_prompt(prompt: &str, model: &str) -> String {
    let wan = dashscope_reference_mode(&model.to_lowercase());
    let chars = prompt.chars().collect::<Vec<_>>();
    let mut result = String::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != '@' || chars.get(index + 1) != Some(&'图') {
            result.push(chars[index]);
            index += 1;
            continue;
        }
        let marker_start = index;
        index += 2;
        while chars.get(index).is_some_and(|value| value.is_whitespace()) {
            index += 1;
        }
        let digits_start = index;
        while chars.get(index).is_some_and(|value| value.is_ascii_digit()) {
            index += 1;
        }
        let digits = chars[digits_start..index].iter().collect::<String>();
        if digits.is_empty() {
            result.push(chars[marker_start]);
            index = marker_start + 1;
        } else if wan {
            result.push_str(&format!("图{digits}"));
        } else {
            result.push_str(&format!("[Image {digits}]"));
        }
    }
    result
}

/// Report whether the selected DashScope model takes its reference media through `reference_urls`.
pub(super) fn dashscope_reference_mode(model: &str) -> bool {
    (model.contains("happyhorse") && model.contains("-r2v"))
        || model.starts_with("wan2.6-r2v")
        || model.starts_with("wan2.7-r2v")
}

/// Return the documented maximum number of reference assets accepted by each supported R2V family.
pub(super) fn dashscope_reference_limit(model: &str) -> usize {
    if model.starts_with("wan2.7-r2v") {
        5
    } else {
        9
    }
}

#[cfg(test)]
mod tests {
    use super::{dashscope_prompt, dashscope_reference_limit, dashscope_reference_mode};

    #[test]
    fn wan_26_reference_models_use_reference_urls_and_character_markers() {
        assert!(dashscope_reference_mode("wan2.6-r2v-flash"));
        assert_eq!(dashscope_reference_limit("wan2.6-r2v-flash"), 9);
        assert_eq!(
            dashscope_prompt("@图 2 走向@图3", "wan2.6-r2v-flash"),
            "图2 走向图3"
        );
    }
}
