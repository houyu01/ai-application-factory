//! Bootstrap restart service actions triggered from failed or cancelled project UI states.

use serde_json::Value;

use crate::error::{AppError, AppResult};

use super::DesktopService;

impl DesktopService {
    /// Resume the latest failed/cancelled screenplay task, preserving its input snapshot and checkpoints.
    pub fn retry_decomposition(&self, project: &str) -> AppResult<Value> {
        match self
            .repository
            .retry_drama_task(project, "script_decomposition")
        {
            Ok(task) => Ok(task),
            Err(AppError::Conflict(_)) => self
                .repository
                .retry_drama_task(project, "script_expansion"),
            Err(error) => Err(error),
        }
    }

    /// Create a new bootstrap run from the original project script after a user cancels the prior run.
    pub fn restart_decomposition(&self, project: &str) -> AppResult<Value> {
        self.repository
            .restart_drama_task(project, "script_decomposition")
    }

    /// Start a new bootstrap from the editor's original screenplay and discard every derived asset, shot, and video history.
    pub fn regenerate_decomposition(
        &self,
        project: &str,
        script: Option<&str>,
    ) -> AppResult<Value> {
        self.repository.regenerate_drama(project, script)
    }
}
