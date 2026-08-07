"""Retry flow for durable short-drama screenplay expansion tasks."""

from __future__ import annotations

from ..domain.models import GenerationStatus


class TaskServiceRetryMixin:
    """Restart a failed project bootstrap without discarding its screenplay checkpoint.

    The failure banner calls this slice when a creator chooses to resume a
    long-form screenplay. It owns task-state validation; the repository keeps
    the task snapshot, while the durable worker performs the resumed work.
    """

    def retry_script_decomposition(self, project_id: str) -> dict:
        """Requeue the latest failed bootstrap task for the project's worker."""

        self.get_project(project_id)
        tasks = self.repository.list_task_statuses(project_id)
        bootstrap_tasks = [task for task in tasks if task.get("type") == "script_decomposition"]
        latest_task = bootstrap_tasks[-1] if bootstrap_tasks else None
        if latest_task is None:
            raise KeyError(f"Script decomposition task not found for project: {project_id}")
        if latest_task.get("status") != GenerationStatus.FAILED.value:
            raise ValueError("剧本任务尚未失败，不能重新开始")
        task = self.repository.retry_failed_task(str(latest_task["id"]))
        self.repository.set_drama_status(project_id, GenerationStatus.GENERATING)
        return task
