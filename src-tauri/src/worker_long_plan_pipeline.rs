//! Pipelined long-drama decomposition that overlaps a batch's asset catalog with the next batch's storyboard.

use std::thread::{self, JoinHandle};

use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    planner,
    providers::ProviderClient,
    skills,
};

use super::{
    decomposition_checkpoint::{
        decomposition_checkpoint, pending_inventory_range, save_decomposition_checkpoint,
        EPISODES_PER_DECOMPOSITION_BATCH,
    },
    expansion::support,
    long_plan::{
        asset_inventory_prompt, batch_progress, batch_source, batch_stage, episode_number,
        long_shots, unique_assets, valid_asset,
    },
    DurableWorker,
};

/// Holds the material catalog that continues after its storyboard output has been checkpointed.
struct PendingBatch {
    inventory: JoinHandle<Vec<Value>>,
}

impl DurableWorker {
    /// Run long-drama decomposition as a one-batch pipeline.
    ///
    /// The persisted drama task owns this flow. A batch's main storyboard result supplies
    /// provisional character continuity immediately, while its independent material catalog
    /// runs in a thread during the following storyboard request. A batch reaches the durable
    /// checkpoint immediately after the storyboard response, then records its optional material
    /// catalog independently. This keeps restart recovery from repeating a completed storyboard
    /// batch while allowing the catalog request to overlap the following batch.
    pub(super) fn run_decomposition_batch_pipeline(
        &self,
        task_id: &str,
        project: &Value,
        screenplay: &str,
    ) -> AppResult<Value> {
        let target = support::target_episode_count(project)?;
        let sections = support::episode_sections(screenplay);
        if sections.len() < target as usize {
            return Err(AppError::BadRequest(format!(
                "扩写剧本不足{target}集，不能进入长剧分镜"
            )));
        }
        let model = project["language_model"].as_str();
        let enable_web = crate::value::bool_value(&project["enable_web_search"]);
        let batches = &sections[..target as usize];
        let batch_count = (batches.len() + EPISODES_PER_DECOMPOSITION_BATCH - 1)
            / EPISODES_PER_DECOMPOSITION_BATCH;
        let mut snapshot = self.repository.get_drama_task(task_id)?["input_snapshot"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        let (completed, mut episodes, mut assets, mut received_chars) =
            decomposition_checkpoint(&snapshot, batches);
        let decomposer = skills::drama_skill(
            "script_decomposer",
            json!({"shot_script_max_chars": project["shot_script_max_chars"].as_i64().unwrap_or(400)}),
        )?;
        let instruction = decomposer["instruction"].as_str().unwrap_or_default();
        let inventory_system = format!(
            "{}\n\n你只负责素材盘点。严格输出合法 JSON，不要 Markdown。\n\n拆解执行规范：\n{}",
            skills::drama_agent_system()?,
            instruction,
        );
        let mut pending =
            pending_inventory_range(&snapshot, batches, completed).map(|(start, end)| {
                PendingBatch {
                    inventory: spawn_inventory(
                        self.providers.clone(),
                        model.map(str::to_owned),
                        inventory_system.clone(),
                        asset_inventory_prompt(project, &batches[start..end]),
                        batch_source(&batches[start..end]),
                        enable_web,
                    ),
                }
            });
        for (relative_index, batch) in batches[completed..]
            .chunks(EPISODES_PER_DECOMPOSITION_BATCH)
            .enumerate()
        {
            if pending
                .as_ref()
                .is_some_and(|previous: &PendingBatch| previous.inventory.is_finished())
            {
                self.commit_pending_inventory(
                    task_id,
                    &mut snapshot,
                    &mut episodes,
                    &mut assets,
                    &mut received_chars,
                    pending.take().expect("checked pending batch"),
                )?;
            }
            let known_characters = character_names(&assets);
            let prior_received = received_chars;
            let batch_index = (completed + relative_index * EPISODES_PER_DECOMPOSITION_BATCH)
                / EPISODES_PER_DECOMPOSITION_BATCH;
            let progress = batch_progress(
                completed + relative_index * EPISODES_PER_DECOMPOSITION_BATCH,
                batches.len(),
            );
            let response = match self.stream_decomposition_batch(
                task_id,
                model,
                project,
                batch,
                &known_characters,
                batch_index,
                batch_count,
                progress,
                prior_received,
                instruction,
                enable_web,
            ) {
                Ok(response) => response,
                Err(error) => {
                    if let Some(previous) = pending.take() {
                        self.commit_pending_inventory(
                            task_id,
                            &mut snapshot,
                            &mut episodes,
                            &mut assets,
                            &mut received_chars,
                            previous,
                        )?;
                    }
                    return Err(error);
                }
            };
            let received = response
                .as_deref()
                .map(|text| text.chars().count())
                .unwrap_or_default();
            self.repository.update_drama_task_progress(
                task_id,
                progress,
                &batch_stage(batch, batch_index, batch_count, received, prior_received),
            )?;
            let parsed = response
                .as_deref()
                .and_then(planner::parse_json_object)
                .unwrap_or_else(|| json!({}));
            let current_episodes = normalized_episodes(&parsed, batch, project);
            let current_assets = model_assets(&parsed, &batch_source(batch));
            if let Some(previous) = pending.take() {
                self.commit_pending_inventory(
                    task_id,
                    &mut snapshot,
                    &mut episodes,
                    &mut assets,
                    &mut received_chars,
                    previous,
                )?;
            }
            episodes.extend(current_episodes);
            assets.extend(current_assets);
            received_chars += received;
            save_decomposition_checkpoint(
                &mut snapshot,
                episodes.len(),
                &episodes,
                &assets,
                received_chars,
                Some(episodes.len()),
            );
            self.repository
                .update_drama_task_snapshot(task_id, Value::Object(snapshot.clone()))?;
            pending = Some(PendingBatch {
                inventory: spawn_inventory(
                    self.providers.clone(),
                    model.map(str::to_owned),
                    inventory_system.clone(),
                    asset_inventory_prompt(project, batch),
                    batch_source(batch),
                    enable_web,
                ),
            });
        }
        if let Some(previous) = pending {
            self.commit_pending_inventory(
                task_id,
                &mut snapshot,
                &mut episodes,
                &mut assets,
                &mut received_chars,
                previous,
            )?;
        }
        assets.extend(planner::extracted_assets(
            screenplay,
            project["theme"].as_str().unwrap_or("都市"),
        ));
        Ok(json!({"episodes":episodes,"assets":unique_assets(assets)}))
    }

    fn commit_pending_inventory(
        &self,
        task_id: &str,
        snapshot: &mut Map<String, Value>,
        episodes: &mut Vec<Value>,
        assets: &mut Vec<Value>,
        received_chars: &mut usize,
        pending: PendingBatch,
    ) -> AppResult<()> {
        let inventory = pending.inventory.join().unwrap_or_default();
        assets.extend(inventory);
        save_decomposition_checkpoint(
            snapshot,
            episodes.len(),
            episodes,
            assets,
            *received_chars,
            None,
        );
        self.repository
            .update_drama_task_snapshot(task_id, Value::Object(snapshot.clone()))
    }
}

fn spawn_inventory(
    providers: ProviderClient,
    model: Option<String>,
    system: String,
    prompt: String,
    source: String,
    enable_web_search: bool,
) -> JoinHandle<Vec<Value>> {
    thread::spawn(move || {
        providers
            .complete_with_web_search(
                "language",
                model.as_deref(),
                &system,
                &prompt,
                enable_web_search,
            )
            .ok()
            .flatten()
            .as_deref()
            .and_then(planner::parse_json_object)
            .and_then(|inventory| inventory["assets"].as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| valid_asset(&item, &planner::AssetEvidence::from_script(&source)))
            .collect()
    })
}

fn normalized_episodes(parsed: &Value, batch: &[support::Episode], project: &Value) -> Vec<Value> {
    batch
        .iter()
        .enumerate()
        .map(|(index, section)| {
            let raw = parsed["episodes"]
                .as_array()
                .and_then(|items| {
                    items
                        .iter()
                        .find(|item| episode_number(item) == Some(section.number))
                        .or_else(|| items.get(index))
                })
                .unwrap_or(&Value::Null);
            json!({"name":format!("第{}集：{}",section.number,section.title),"shots":long_shots(raw, section, project)})
        })
        .collect()
}

fn model_assets(parsed: &Value, source: &str) -> Vec<Value> {
    let evidence = planner::AssetEvidence::from_script(source);
    parsed["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| valid_asset(item, &evidence))
        .collect()
}

fn character_names(assets: &[Value]) -> Vec<String> {
    assets
        .iter()
        .filter(|asset| asset["type"] == "character")
        .filter_map(|asset| asset["name"].as_str().map(str::to_owned))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::character_names;

    #[test]
    fn next_batch_uses_the_durably_saved_primary_assets() {
        let names = character_names(&[
            json!({"type":"character","name":"已存角色"}),
            json!({"type":"character","name":"当前批角色"}),
        ]);
        assert_eq!(names, ["已存角色", "当前批角色"]);
    }
}
