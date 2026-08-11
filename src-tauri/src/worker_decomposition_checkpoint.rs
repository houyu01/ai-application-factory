//! Durable per-batch checkpointing for long-form storyboard decomposition.

use serde_json::{json, Map, Value};

use super::expansion::support;

/// Keep model requests small enough to return a visible first token quickly.
pub(super) const EPISODES_PER_DECOMPOSITION_BATCH: usize = 3;
const LEGACY_EPISODES_PER_DECOMPOSITION_BATCH: usize = 10;

/// Recover only complete storyboard output, preserving checkpoints written before the batch-size reduction.
pub(super) fn decomposition_checkpoint(
    snapshot: &Map<String, Value>,
    batches: &[support::Episode],
) -> (usize, Vec<Value>, Vec<Value>, usize) {
    let Some(checkpoint) = snapshot
        .get("decomposition_checkpoint")
        .and_then(Value::as_object)
    else {
        return (0, Vec::new(), Vec::new(), 0);
    };
    let episodes = checkpoint
        .get("episodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let completed = checkpoint
        .get("completed_episodes")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or_else(|| {
            let batch_size = checkpoint
                .get("batch_size")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .filter(|value| *value > 0)
                .unwrap_or(LEGACY_EPISODES_PER_DECOMPOSITION_BATCH);
            let completed_batches = checkpoint
                .get("completed_batches")
                .and_then(Value::as_u64)
                .unwrap_or_default() as usize;
            completed_batches
                .saturating_mul(batch_size)
                .min(batches.len())
        });
    if completed > batches.len() || episodes.len() != completed {
        return (0, Vec::new(), Vec::new(), 0);
    }
    let assets = checkpoint
        .get("assets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let received_chars = checkpoint
        .get("received_chars")
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize;
    (completed, episodes, assets, received_chars)
}

/// Locate the one material catalog that was still running when its storyboard batch was checkpointed.
pub(super) fn pending_inventory_range(
    snapshot: &Map<String, Value>,
    batches: &[support::Episode],
    completed_episodes: usize,
) -> Option<(usize, usize)> {
    let checkpoint = snapshot.get("decomposition_checkpoint")?.as_object()?;
    let inventory_through = checkpoint.get("inventory_pending_through")?.as_u64()? as usize;
    if inventory_through == 0
        || inventory_through != completed_episodes
        || inventory_through > batches.len()
    {
        return None;
    }
    let start = (inventory_through - 1) / EPISODES_PER_DECOMPOSITION_BATCH
        * EPISODES_PER_DECOMPOSITION_BATCH;
    (start < inventory_through).then_some((start, inventory_through))
}

/// Store completed planning output inside the durable bootstrap task before the next provider request begins.
pub(super) fn save_decomposition_checkpoint(
    snapshot: &mut Map<String, Value>,
    completed_episodes: usize,
    episodes: &[Value],
    assets: &[Value],
    received_chars: usize,
    inventory_pending_through: Option<usize>,
) {
    snapshot.insert(
        "decomposition_checkpoint".to_owned(),
        json!({
            "batch_size": EPISODES_PER_DECOMPOSITION_BATCH,
            "completed_episodes": completed_episodes,
            "episodes": episodes,
            "assets": assets,
            "received_chars": received_chars,
            "inventory_pending_through": inventory_pending_through,
        }),
    );
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Map};

    use super::{decomposition_checkpoint, pending_inventory_range, save_decomposition_checkpoint};
    use crate::worker::expansion::support::Episode;

    #[test]
    fn resumes_only_when_all_episodes_of_completed_batches_are_saved() {
        let batches = (1..=11)
            .map(|number| Episode {
                number,
                title: format!("第{number}集"),
                body: "正文".to_owned(),
            })
            .collect::<Vec<_>>();
        let snapshot = Map::from_iter([(
            "decomposition_checkpoint".to_owned(),
            json!({
                "completed_episodes": 3,
                "episodes": vec![json!({}); 3],
                "assets": [json!({"name":"林满"})],
                "received_chars": 16031,
            }),
        )]);

        let (completed, episodes, assets, received_chars) =
            decomposition_checkpoint(&snapshot, &batches);

        assert_eq!(completed, 3);
        assert_eq!(episodes.len(), 3);
        assert_eq!(assets.len(), 1);
        assert_eq!(received_chars, 16031);
    }

    #[test]
    fn discards_partial_or_mismatched_checkpoints() {
        let batches = (1..=11)
            .map(|number| Episode {
                number,
                title: format!("第{number}集"),
                body: "正文".to_owned(),
            })
            .collect::<Vec<_>>();
        let snapshot = Map::from_iter([(
            "decomposition_checkpoint".to_owned(),
            json!({"completed_episodes": 3, "episodes": [json!({})]}),
        )]);

        assert_eq!(decomposition_checkpoint(&snapshot, &batches).0, 0);
    }

    #[test]
    fn preserves_a_completed_legacy_ten_episode_batch() {
        let batches = (1..=11)
            .map(|number| Episode {
                number,
                title: format!("第{number}集"),
                body: "正文".to_owned(),
            })
            .collect::<Vec<_>>();
        let snapshot = Map::from_iter([(
            "decomposition_checkpoint".to_owned(),
            json!({"completed_batches": 1, "episodes": vec![json!({}); 10]}),
        )]);

        let (completed, episodes, _, _) = decomposition_checkpoint(&snapshot, &batches);

        assert_eq!(completed, 10);
        assert_eq!(episodes.len(), 10);
    }

    #[test]
    fn resumes_a_pending_inventory_without_repeating_its_storyboard_batch() {
        let batches = (1..=7)
            .map(|number| Episode {
                number,
                title: format!("第{number}集"),
                body: "正文".to_owned(),
            })
            .collect::<Vec<_>>();
        let mut snapshot = Map::new();
        save_decomposition_checkpoint(&mut snapshot, 3, &vec![json!({}); 3], &[], 2345, Some(3));

        let (completed, episodes, _, received_chars) =
            decomposition_checkpoint(&snapshot, &batches);

        assert_eq!(completed, 3);
        assert_eq!(episodes.len(), 3);
        assert_eq!(received_chars, 2345);
        assert_eq!(
            pending_inventory_range(&snapshot, &batches, completed),
            Some((0, 3))
        );
    }
}
