//! Independent durable queues matching Python's language, image, video, and audio worker families.

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    thread,
    time::Duration,
};

use crate::{error::AppResult, repository::Repository};

use super::DurableWorker;

const MAX_CONCURRENCY: usize = 8;
const QUEUE_MODEL_KINDS: [&str; 4] = ["language", "multimodal", "video", "audio"];
const QUEUE_TASK_TYPES: [&[&str]; 4] = [
    &[
        "script_decomposition",
        "script_expansion",
        "shot_prompt",
        "shot_quality",
    ],
    &[
        "asset_image",
        "asset_variant_image",
        "asset_image_batch",
        "shot_reference_image_batch",
        "placeholder_image",
        "cover_image",
    ],
    &["shot_video", "drama_video_export"],
    &["audio_generation"],
];

/// Holds current per-provider limits and tracks live slot threads across Settings changes.
pub(super) struct QueueControl {
    limits: [AtomicUsize; 4],
    started: Mutex<[[bool; MAX_CONCURRENCY]; 4]>,
}

impl QueueControl {
    pub(super) fn from_repository(repository: &Repository) -> AppResult<Self> {
        let limits = QUEUE_MODEL_KINDS.map(|kind| {
            repository
                .setting(kind)
                .ok()
                .and_then(|value| value["generation_concurrency"].as_u64())
                .map(|value| value.clamp(1, MAX_CONCURRENCY as u64) as usize)
                .unwrap_or(2)
        });
        Ok(Self {
            limits: limits.map(AtomicUsize::new),
            started: Mutex::new([[false; MAX_CONCURRENCY]; 4]),
        })
    }
}

/// Start only the configured slots; later Settings increases add the missing slots without restarting the app.
pub(super) fn start(worker: &DurableWorker) {
    for index in 0..QUEUE_MODEL_KINDS.len() {
        start_slots(worker, index);
    }
}

/// Apply the persisted model-card concurrency to its queue and wake new slots when the count grows.
pub(super) fn set_concurrency(worker: &DurableWorker, model_kind: &str, concurrency: usize) {
    let Some(queue) = QUEUE_MODEL_KINDS
        .iter()
        .position(|kind| *kind == model_kind)
    else {
        return;
    };
    worker.queues.limits[queue].store(concurrency.clamp(1, MAX_CONCURRENCY), Ordering::Relaxed);
    start_slots(worker, queue);
}

fn start_slots(worker: &DurableWorker, queue: usize) {
    let limit = worker.queues.limits[queue].load(Ordering::Relaxed);
    let mut started = worker
        .queues
        .started
        .lock()
        .expect("queue slots are available");
    for slot in 0..limit {
        if started[queue][slot] {
            continue;
        }
        started[queue][slot] = true;
        let clone = worker.clone();
        thread::spawn(move || run_slot(clone, queue, slot));
    }
}

fn run_slot(worker: DurableWorker, queue: usize, slot: usize) {
    while worker.running.load(Ordering::Relaxed) {
        if slot >= worker.queues.limits[queue].load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(500));
            continue;
        }
        let did_work = worker
            .repository
            .claim_drama_task_types(QUEUE_TASK_TYPES[queue])
            .map(|task| {
                if let Some(task) = task {
                    worker.run_drama(task);
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false);
        let game_types = match queue {
            0 => &["game_script_expansion", "game_graph_decomposition"][..],
            1 => &[
                "game_asset_image",
                "game_asset_variant_image",
                "game_cover_image",
                "game_placeholder_image",
            ][..],
            2 => &["node_video_generation"][..],
            _ => &[],
        };
        let game_work = if !did_work {
            worker
                .repository
                .claim_game_task_types(game_types)
                .map(|task| {
                    if let Some(task) = task {
                        worker.run_game(task);
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false)
        } else {
            false
        };
        let voice_work = if !did_work && !game_work && queue == 3 {
            worker
                .repository
                .claim_voice_audio_task()
                .map(|task| {
                    if let Some(task) = task {
                        worker.run_voice_audio(task);
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false)
        } else {
            false
        };
        if !did_work && !game_work && !voice_work {
            thread::sleep(Duration::from_millis(500));
        }
    }
}
