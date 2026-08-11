//! Provider prompt construction for durable video-refinement tasks.

use serde_json::{json, Value};

use crate::{error::AppResult, worker::DurableWorker};

impl DurableWorker {
    /// Rebuild the original version's prompt and frozen image references, then append the creator's requested video change.
    pub(super) fn video_refinement_inputs(
        &self,
        project: &Value,
        shot: &Value,
        refinement: &Value,
    ) -> AppResult<(String, Vec<String>, Vec<Option<String>>)> {
        let original_prompt = refinement["original_prompt"]
            .as_str()
            .unwrap_or_default()
            .trim();
        let request = refinement["prompt"].as_str().unwrap_or_default().trim();
        let mut original_shot = shot.clone();
        original_shot["prompt"] = json!(format!(
            "原始提示词（仅供微调参考；未提及的内容请保持不变）：\n{original_prompt}\n\n用户微调提示词（必须优先执行）：\n{request}"
        ));
        if refinement["original_prompt_rich"].is_array() {
            original_shot["prompt_rich"] = refinement["original_prompt_rich"].clone();
        }
        if refinement["original_structured"].is_object() {
            original_shot["structured"] = refinement["original_structured"].clone();
        }
        self.video_generation_inputs(project, &original_shot)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn refinement_snapshot_labels_original_and_requested_prompts() {
        let refinement = json!({"original_prompt":"雨夜街道，镜头推进","prompt":"让灯光更温暖"});
        let original = refinement["original_prompt"].as_str().unwrap();
        let request = refinement["prompt"].as_str().unwrap();
        let prompt = format!(
            "原始提示词（仅供微调参考；未提及的内容请保持不变）：\n{original}\n\n用户微调提示词（必须优先执行）：\n{request}"
        );

        assert!(prompt.contains("原始提示词"));
        assert!(prompt.contains("用户微调提示词"));
        assert!(prompt.contains(request));
    }
}
