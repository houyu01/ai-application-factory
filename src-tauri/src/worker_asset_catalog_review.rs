//! Constrained language-model review for the source-grounded material catalog.

use serde_json::{json, Value};

use crate::{planner, skills};

use super::DurableWorker;

impl DurableWorker {
    /// Ask the configured language model to classify, rename, merge, or drop catalog entries.
    ///
    /// The decomposition task calls this after deterministic review and before persistence. Model
    /// failure or malformed output preserves the deterministic catalog, so creation remains usable
    /// offline and a reviewer can never invent an entity that is absent from the screenplay.
    pub(super) fn review_asset_catalog_with_model(
        &self,
        project: &Value,
        screenplay: &str,
        assets: Vec<Value>,
    ) -> Vec<Value> {
        if assets.is_empty() {
            return assets;
        }
        let system = match skills::drama_agent_system() {
            Ok(value) => format!(
                "{value}\n\n你是素材目录校对员，只纠正分类、名称边界、重复项；严格返回 JSON。"
            ),
            Err(_) => return assets,
        };
        let prompt = catalog_review_prompt(screenplay, &assets);
        let response = self.providers.complete_with_web_search(
            "language",
            project["language_model"].as_str(),
            &system,
            &prompt,
            false,
        );
        response
            .ok()
            .flatten()
            .as_deref()
            .and_then(planner::parse_json_object)
            .and_then(|value| apply_decisions(&assets, &value))
            .unwrap_or(assets)
    }
}

fn catalog_review_prompt(screenplay: &str, assets: &[Value]) -> String {
    let catalog = assets
        .iter()
        .enumerate()
        .map(|(index, asset)| {
            json!({
                "index": index,
                "type": asset["type"],
                "name": asset["name"],
                "context": evidence_context(screenplay, asset["name"].as_str().unwrap_or_default()),
            })
        })
        .collect::<Vec<_>>();
    format!(
        "复核以下人物、场景、道具目录。人物必须是有生命且参与剧情的实体；箱包、文具、武器、证件、衣物等属于道具。检查姓名是否粘连了量词、动作或相邻正文，例如已有‘王德福’时不得另保留‘王德福一’。合并重复别名。\n\n只能针对给定 index 输出决定，不得新增条目，不得改写 prompt。action 只能是 keep、drop、rename、move、merge；rename/move 的 name 必须逐字出现在该项 context；move 的 type 只能是 character、scene、prop；merge 必须填写 target_index。无法确定就 keep。每个 index 必须恰好出现一次。只返回 {{\"decisions\":[{{\"index\":0,\"action\":\"keep\",\"type\":\"character\",\"name\":\"原名\",\"target_index\":null}}]}}。\n\n目录：\n{}",
        serde_json::to_string(&catalog).unwrap_or_else(|_| "[]".to_owned())
    )
}

fn evidence_context(script: &str, name: &str) -> String {
    let Some(offset) = script.find(name) else {
        return String::new();
    };
    let before = script[..offset].chars().rev().take(48).collect::<String>();
    let before = before.chars().rev().collect::<String>();
    let after = script[offset + name.len()..]
        .chars()
        .take(64)
        .collect::<String>();
    format!("{before}{name}{after}")
}

fn apply_decisions(assets: &[Value], response: &Value) -> Option<Vec<Value>> {
    let decisions = response["decisions"].as_array()?;
    if decisions.len() != assets.len() {
        return None;
    }
    let mut seen = vec![false; assets.len()];
    let mut output = Vec::new();
    for decision in decisions {
        let index = decision["index"].as_u64()? as usize;
        if index >= assets.len() || seen[index] {
            return None;
        }
        seen[index] = true;
        let action = decision["action"].as_str()?;
        if action == "drop" || action == "merge" {
            if action == "merge" && decision["target_index"].as_u64().is_none() {
                return None;
            }
            continue;
        }
        let mut asset = assets[index].clone();
        if matches!(action, "rename" | "move") {
            let name = decision["name"].as_str()?.trim();
            let context = evidence_context_for_asset(assets, index, name);
            if name.is_empty() || !context {
                return None;
            }
            asset["name"] = json!(name);
        } else if action != "keep" {
            return None;
        }
        if action == "move" {
            let kind = decision["type"].as_str()?;
            if !["character", "scene", "prop"].contains(&kind) {
                return None;
            }
            asset["type"] = json!(kind);
        }
        output.push(asset);
    }
    seen.into_iter().all(|value| value).then_some(output)
}

fn evidence_context_for_asset(assets: &[Value], index: usize, proposed: &str) -> bool {
    let original = assets[index]["name"].as_str().unwrap_or_default();
    original.contains(proposed) || proposed.contains(original)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::apply_decisions;

    #[test]
    fn constrained_review_moves_object_and_merges_stuck_name_tail() {
        let assets = vec![
            json!({"type":"character","name":"皮公文包","prompt":"皮质包"}),
            json!({"type":"character","name":"王德福","prompt":"村长"}),
            json!({"type":"character","name":"王德福一","prompt":"村长"}),
        ];
        let response = json!({"decisions":[
            {"index":0,"action":"move","type":"prop","name":"皮公文包"},
            {"index":1,"action":"keep"},
            {"index":2,"action":"merge","target_index":1}
        ]});
        let reviewed = apply_decisions(&assets, &response).expect("valid decisions");
        assert_eq!(reviewed.len(), 2);
        assert_eq!(reviewed[0]["type"], "prop");
        assert_eq!(reviewed[1]["name"], "王德福");
    }
}
