//! Short-drama-compatible rich-prompt generation for one interactive-game video node.

use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    planner,
    value::SUCCEEDED,
};

use super::{
    prompt_helpers::{append_missing_references, filter_disallowed_sections, rich_prompt_request},
    DurableWorker,
};

const GAME_NODE_TRANSITION_PROMPT: &str = "互动视频节点衔接（最高优先级）：每个视频节点都必须遵循“来源 original_text → 选择边 option_text → 当前 original_text → 下一条选择边”的剧情链。入边选择是玩家在上段视频结束时作出的动作、回答、立场或信息处置；本节点开场必须从该选择的执行和即时后果自然开始。出边选择只能在本节点原文结尾形成待玩家决定的抉择，绝不可在本视频内替玩家提前执行。提示词必须新增“前序承接：”和“选择后果：”段，并把传入的衔接上下文与当前 original_text 共同画面化；不得用节点标题、泛化转场或无因跳场替代选择边造成的因果。";

impl DurableWorker {
    /// Decompose one game node into editable rich prompt text and owned image-reference chips.
    pub(super) fn game_node_prompt(
        &self,
        task_id: &str,
        game_id: &str,
        task: &Value,
    ) -> AppResult<()> {
        let node_id = task["resource_id"].as_str().unwrap_or_default();
        let game = self.repository.get_game(game_id)?;
        let node = self.repository.get_game_node(game_id, node_id)?;
        let version = node["prompt_template_version"].as_str().unwrap_or("v1");
        let template = self
            .repository
            .prompt_templates("drama", Some("shot_prompt"), false)?
            .into_iter()
            .find(|item| item["version"].as_str() == Some(version));
        let template = template.or_else(|| {
            self.repository
                .prompt_templates("drama", Some("shot_prompt"), false)
                .ok()
                .and_then(|items| items.into_iter().next())
        });
        let version = template
            .as_ref()
            .and_then(|item| item["version"].as_str())
            .unwrap_or(version);
        let system = format!(
            "{}\n\n{GAME_NODE_TRANSITION_PROMPT}",
            template
                .as_ref()
                .and_then(|item| item["template_text"].as_str())
                .unwrap_or("返回可编辑分镜富提示词 JSON。")
        );
        let references =
            selected_game_references(&node, game["assets"].as_array().unwrap_or(&Vec::new()));
        let context = prompt_context(&game);
        let mut prompt_node = node.clone();
        prompt_node["prompt_template_version"] = json!(version);
        prompt_node["transition_context"] = json!(game_node_transition_context(&game, &node));
        let response = self
            .providers
            .complete_with_web_search(
                "language",
                game["language_model"].as_str(),
                &system,
                &rich_prompt_request(&context, &prompt_node, &references, version),
                crate::value::bool_value(&game["enable_web_search"]),
            )?
            .ok_or_else(|| {
                AppError::External("语言模型未返回该节点的提示词，请仅重试当前节点。".to_owned())
            })?;
        let mut nodes = planner::model_rich_prompt(&response, &references).ok_or_else(|| {
            AppError::External(
                "语言模型返回的节点提示词格式无效，未覆盖当前提示词；请仅重试当前节点。".to_owned(),
            )
        })?;
        nodes = append_missing_references(nodes, &references);
        nodes = filter_disallowed_sections(nodes, &context);
        let prompt = planner::prompt_text(&nodes);
        self.repository
            .save_generated_game_node_prompt(game_id, node_id, &prompt, &nodes, version)?;
        self.repository.finish_game_task(
            task_id,
            SUCCEEDED,
            Some(json!({
                "node_id": node_id,
                "prompt": prompt,
                "prompt_rich": nodes,
                "reference_asset_ids": nodes.iter().filter(|node| node["type"] == "reference")
                    .filter_map(|node| node["asset_id"].as_str()).collect::<Vec<_>>(),
            })),
            None,
        )?;
        Ok(())
    }
}

fn prompt_context(game: &Value) -> Value {
    let mut context = game.clone();
    let object = context.as_object_mut().expect("game is an object");
    object.insert(
        "ratio".to_owned(),
        json!(if game["platform"].as_str() == Some("Steam游戏") {
            "16:9"
        } else {
            "9:16"
        }),
    );
    object.insert(
        "shot_constraints".to_owned(),
        json!({"subtitles":false,"background_music":false}),
    );
    context
}

fn selected_game_references(node: &Value, assets: &[Value]) -> Vec<Value> {
    let mut selected = Vec::new();
    for rich in node["prompt_rich"].as_array().into_iter().flatten() {
        add_reference(
            &mut selected,
            assets,
            rich["asset_id"].as_str(),
            rich["variant_id"].as_str(),
        );
    }
    for id in node["reference_asset_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        add_reference(&mut selected, assets, Some(id), None);
    }
    add_reference(
        &mut selected,
        assets,
        node["placeholder_asset_id"].as_str(),
        None,
    );
    let source = format!(
        "{} {}",
        node["title"].as_str().unwrap_or_default(),
        node["original_text"].as_str().unwrap_or_default()
    );
    for asset in assets {
        let name = asset["name"].as_str().unwrap_or_default();
        if !name.is_empty()
            && (source.contains(name)
                || name
                    .split(['·', '/', '、', '，', ' ', '：'])
                    .any(|term| term.chars().count() >= 2 && source.contains(term)))
        {
            add_resolved_reference(&mut selected, asset.clone());
        }
    }
    for kind in ["scene", "character", "prop"] {
        if !selected.iter().any(|asset| asset["type"] == kind) {
            if let Some(asset) = assets.iter().find(|asset| asset["type"] == kind) {
                add_resolved_reference(&mut selected, asset.clone());
            }
        }
    }
    selected
}

fn add_reference(
    selected: &mut Vec<Value>,
    assets: &[Value],
    asset_id: Option<&str>,
    variant_id: Option<&str>,
) {
    let Some(asset_id) = asset_id.filter(|value| !value.is_empty()) else {
        return;
    };
    if let Some(asset) = planner::resolve_reference_asset(assets, asset_id, variant_id) {
        add_resolved_reference(selected, asset);
    }
}

fn add_resolved_reference(selected: &mut Vec<Value>, asset: Value) {
    if !matches!(
        asset["type"].as_str(),
        Some("scene") | Some("character") | Some("prop") | Some("placeholder")
    ) {
        return;
    }
    let key = planner::reference_key(
        asset["id"].as_str().unwrap_or_default(),
        asset["variant_id"].as_str(),
    );
    if !selected.iter().any(|current| {
        planner::reference_key(
            current["id"].as_str().unwrap_or_default(),
            current["variant_id"].as_str(),
        ) == key
    }) {
        selected.push(asset);
    }
}

fn game_node_transition_context(game: &Value, node: &Value) -> String {
    let node_id = node["id"].as_str().unwrap_or_default();
    let original = node["original_text"].as_str().unwrap_or_default();
    let node_details = |id: &str| {
        game["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|candidate| candidate["id"].as_str() == Some(id))
            .map(|candidate| {
                format!(
                    "「{}」原始文本：{}",
                    candidate["title"].as_str().unwrap_or(id),
                    candidate["original_text"].as_str().unwrap_or_default()
                )
            })
            .unwrap_or_else(|| format!("节点「{id}」"))
    };
    let edges = game["edges"].as_array().into_iter().flatten();
    let incoming = edges
        .clone()
        .filter(|edge| edge["target_node_id"].as_str() == Some(node_id))
        .map(|edge| {
            format!(
                "{} → 玩家选择「{}」→ 当前原始文本：{}；本节点开场必须呈现该选择的执行与即时后果。",
                node_details(edge["source_node_id"].as_str().unwrap_or_default()),
                edge["option_text"].as_str().unwrap_or_default(),
                original
            )
        })
        .collect::<Vec<_>>();
    let outgoing = edges
        .filter(|edge| edge["source_node_id"].as_str() == Some(node_id))
        .map(|edge| {
            format!(
                "当前原始文本结尾 → 玩家选择「{}」→ {}；本节点只铺垫此抉择，不得提前执行。",
                edge["option_text"].as_str().unwrap_or_default(),
                node_details(edge["target_node_id"].as_str().unwrap_or_default())
            )
        })
        .collect::<Vec<_>>();
    format!(
        "入边剧情桥：{}\n出边剧情桥：{}",
        if incoming.is_empty() {
            "无（起始节点）".to_owned()
        } else {
            incoming.join("\n")
        },
        if outgoing.is_empty() {
            "无（终局节点）".to_owned()
        } else {
            outgoing.join("\n")
        },
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::game_node_transition_context;

    #[test]
    fn node_prompt_context_names_both_sides_of_each_choice_bridge() {
        let game = json!({
            "nodes":[
                {"id":"start","title":"入口","original_text":"侦探发现封锁的侧门。"},
                {"id":"target","title":"侧门","original_text":"侦探撬开侧门进入档案室。"},
                {"id":"ending","title":"终局","original_text":"侦探带着档案离开。"}
            ],
            "edges":[
                {"source_node_id":"start","target_node_id":"target","option_text":"撬开封锁的侧门"},
                {"source_node_id":"target","target_node_id":"ending","option_text":"带着档案撤离"}
            ]
        });
        let context = game_node_transition_context(&game, &game["nodes"][1]);

        assert!(context.contains("「入口」原始文本：侦探发现封锁的侧门"));
        assert!(context.contains("玩家选择「撬开封锁的侧门」"));
        assert!(context.contains("当前原始文本：侦探撬开侧门进入档案室"));
        assert!(context.contains("玩家选择「带着档案撤离」"));
        assert!(context.contains("「终局」原始文本：侦探带着档案离开"));
    }
}
