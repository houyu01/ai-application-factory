//! Rich-prompt reference-image ordering and marker mapping for video provider requests.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::planner;

use super::DurableWorker;

impl DurableWorker {
    /// Resolve ordered provider image URLs and retain the character asset that first supplied each image.
    pub(super) fn video_reference_plan(
        &self,
        project: &Value,
        shot: &Value,
    ) -> (Vec<String>, HashMap<i64, usize>, Vec<(String, String)>) {
        let assets = project["assets"].as_array().cloned().unwrap_or_default();
        let mut images = Vec::new();
        let mut marker_indexes = HashMap::new();
        let mut asset_indexes = HashMap::new();
        let mut seen_urls = HashSet::new();
        let mut reference_sources = Vec::new();
        for node in shot["prompt_rich"].as_array().into_iter().flatten() {
            if node["type"] != "reference" {
                continue;
            }
            let id = node["asset_id"].as_str().unwrap_or_default();
            let base_asset = assets.iter().find(|asset| asset["id"].as_str() == Some(id));
            if node["asset_type"].as_str() == Some("placeholder")
                && base_asset.and_then(|asset| asset["metadata"]["render_mode"].as_str())
                    != Some("generated_composite")
            {
                continue;
            }
            let asset = planner::resolve_reference_asset(&assets, id, node["variant_id"].as_str());
            let url = node["snapshot_image_url"]
                .as_str()
                .or_else(|| node["image_url"].as_str())
                .or_else(|| asset.as_ref().and_then(|asset| asset["image_url"].as_str()))
                .and_then(|url| self.media.provider_reference_url(url));
            let Some(url) = url else { continue };
            let key = planner::reference_key(id, node["variant_id"].as_str());
            let index = if let Some(index) = asset_indexes.get(&key) {
                *index
            } else if let Some(index) = images
                .iter()
                .position(|item| item == &url)
                .map(|item| item + 1)
            {
                asset_indexes.insert(key.clone(), index);
                index
            } else if seen_urls.insert(url.clone()) {
                images.push(url);
                let index = images.len();
                asset_indexes.insert(key, index);
                reference_sources.push((images[index - 1].clone(), id.to_owned()));
                index
            } else {
                continue;
            };
            if let Some(marker) = node["mention_number"].as_i64() {
                marker_indexes.entry(marker).or_insert(index);
            }
        }
        (images, marker_indexes, reference_sources)
    }
}
