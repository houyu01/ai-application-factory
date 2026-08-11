//! Lease heartbeat for synchronous provider work owned by the durable task worker.

use std::{
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::Duration,
};

use serde_json::Value;

use crate::repository::Repository;

const LEASE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

/// Keeps a claimed drama task exclusive while a model call runs, without delaying recovery after a crash.
pub(super) struct DramaTaskLease {
    stop: mpsc::Sender<()>,
}

impl DramaTaskLease {
    /// Begin renewing the persisted lease until the worker finishes or defers the task for later polling.
    pub(super) fn start(repository: Repository, task: &Value) -> Option<Self> {
        Self::start_with_interval(repository, task, LEASE_HEARTBEAT_INTERVAL)
    }

    fn start_with_interval(
        repository: Repository,
        task: &Value,
        heartbeat_interval: Duration,
    ) -> Option<Self> {
        let id = task["id"].as_str()?.to_owned();
        let token = task["poll_lease_token"].as_str()?.to_owned();
        if id.is_empty() || token.is_empty() {
            return None;
        }
        if !repository
            .renew_drama_task_lease(&id, &token)
            .unwrap_or(false)
        {
            return None;
        }
        let (stop, receiver) = mpsc::channel();
        let renewing_repository = repository;
        thread::spawn(move || {
            while matches!(
                receiver.recv_timeout(heartbeat_interval),
                Err(RecvTimeoutError::Timeout)
            ) {
                if !renewing_repository
                    .renew_drama_task_lease(&id, &token)
                    .unwrap_or(false)
                {
                    break;
                }
            }
        });
        Some(Self { stop })
    }
}

impl Drop for DramaTaskLease {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}

/// Keeps a claimed interactive-game expansion or graph task exclusive while its language call runs.
pub(super) struct GameTaskLease {
    stop: mpsc::Sender<()>,
}

impl GameTaskLease {
    /// Begin renewing the game-task lease until the worker reaches a terminal task state.
    pub(super) fn start(repository: Repository, task: &Value) -> Option<Self> {
        let id = task["id"].as_str()?.to_owned();
        let token = task["poll_lease_token"].as_str()?.to_owned();
        if id.is_empty()
            || token.is_empty()
            || !repository
                .renew_game_task_lease(&id, &token)
                .unwrap_or(false)
        {
            return None;
        }
        let (stop, receiver) = mpsc::channel();
        thread::spawn(move || {
            while matches!(
                receiver.recv_timeout(LEASE_HEARTBEAT_INTERVAL),
                Err(RecvTimeoutError::Timeout)
            ) {
                if !repository
                    .renew_game_task_lease(&id, &token)
                    .unwrap_or(false)
                {
                    break;
                }
            }
        });
        Some(Self { stop })
    }
}

impl Drop for GameTaskLease {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, thread};

    use serde_json::{json, Map};

    use super::*;
    use crate::{db::Database, repository::Repository, value::new_id};

    #[test]
    fn heartbeat_renews_a_claimed_task_before_its_recovery_lease_expires() {
        let root = std::env::temp_dir().join(format!("ai-application-factory-{}", new_id()));
        let repository = Repository::new(
            Database::open(root.join("ai_application_factory.db")).expect("test database"),
        );
        repository
            .create_drama(Map::from_iter([
                ("name".to_owned(), json!("心跳租约短剧")),
                (
                    "script".to_owned(),
                    json!("主角收到一封来历不明的信，决定查清真相。"),
                ),
            ]))
            .expect("create project");
        let claimed = repository
            .claim_drama_task_types(&["script_decomposition"])
            .expect("claim task")
            .expect("task available");
        let task_id = claimed["id"].as_str().expect("task id");
        let lease = DramaTaskLease::start_with_interval(
            repository.clone(),
            &claimed,
            Duration::from_millis(1),
        )
        .expect("start heartbeat");
        let first = repository
            .get_drama_task(task_id)
            .expect("initial renewed lease")["poll_lease_until"]
            .clone();

        thread::sleep(Duration::from_millis(20));
        let renewed = repository
            .get_drama_task(task_id)
            .expect("heartbeat renewed lease")["poll_lease_until"]
            .clone();
        assert_ne!(first, renewed);
        drop(lease);
        thread::sleep(Duration::from_millis(5));
        fs::remove_dir_all(root).expect("remove test data");
    }
}
