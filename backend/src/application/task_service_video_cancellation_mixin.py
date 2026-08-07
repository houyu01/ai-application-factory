"""Cancellation flow for durable short-drama video generations."""

from __future__ import annotations

import logging
from typing import Any

from ..domain.models import GenerationStatus


logger = logging.getLogger(__name__)


class TaskServiceVideoCancellationMixin:
    """Stop a shot's running video task from the editor's cancellation menu.

    The shot editor calls this flow after the creator chooses ``取消生成``. It
    owns the durable cancellation boundary: stop local worker progression
    first, then cancel the already-created Volcengine Ark provider task.
    """

    def cancel_shot_video(self, project_id: str, shot_id: str) -> dict[str, Any]:
        """Cancel every active video run for one shot and preserve audit rows."""

        project = self.get_project(project_id)
        if self.repository.get_shot(project_id, shot_id) is None:
            raise KeyError(f"Shot not found: {shot_id}")
        tasks = [
            task for task in self.repository.list_active_tasks(project_id, "shot_video")
            if task.get("resource_id") == shot_id
        ]
        if not tasks:
            raise ValueError("当前分镜没有正在生成的视频任务")
        cancelled_tasks = [self._cancel_video_task(project, task) for task in tasks]
        return {
            **cancelled_tasks[-1],
            "cancelled_count": len(cancelled_tasks),
            "cancelled_tasks": cancelled_tasks,
        }

    def cancel_all_shot_videos(self, project_id: str) -> dict[str, Any]:
        """Cancel every active shot-video task when the top-bar bulk action is clicked."""

        project = self.get_project(project_id)
        cancelled_tasks: list[dict[str, Any]] = []
        provider_cancel_errors: list[dict[str, str]] = []
        for task in self.repository.list_active_tasks(project_id, "shot_video"):
            try:
                cancelled = self._cancel_video_task(project, task)
            except ValueError:
                continue
            cancelled_tasks.append(cancelled)
            if cancelled.get("provider_cancel_error"):
                provider_cancel_errors.append({
                    "task_id": str(cancelled["id"]),
                    "error": str(cancelled["provider_cancel_error"]),
                })
        return {
            "project_id": project_id,
            "cancelled_count": len(cancelled_tasks),
            "cancelled_tasks": cancelled_tasks,
            "provider_cancel_errors": provider_cancel_errors,
        }

    def _cancel_video_task(self, project: dict[str, Any], task: dict[str, Any]) -> dict[str, Any]:
        """Persist one local cancellation before best-effort provider cleanup."""

        project_id = str(task["drama_id"])
        shot_id = str(task["resource_id"])

        cancelled = self.repository.cancel_task(
            str(task["id"]), stage="视频生成已取消"
        )
        self.sync_shot_video_status(
            project_id, shot_id, GenerationStatus.CANCELLED
        )
        self._cancel_shot_video_version(task)

        provider_task_id = str(task.get("provider_task_id") or "").strip()
        result: dict[str, Any] = dict(cancelled)
        if not provider_task_id:
            result["provider_cancelled"] = False
            return result
        try:
            provider_cancelled = self._cancel_remote_video_task(project, provider_task_id)
            result["provider_cancelled"] = provider_cancelled is not False
            if provider_cancelled is False:
                result["provider_cancel_error"] = (
                    "当前视频服务商未提供远端取消接口，平台已停止轮询并标记任务为已取消。"
                )
        except Exception as exc:
            # The local task remains cancelled, so a failed remote request can
            # never make this video appear in the product after a refresh.
            result["provider_cancelled"] = False
            result["provider_cancel_error"] = str(exc)
            logger.warning(
                "Failed to cancel Ark video task %s for drama task %s: %s",
                provider_task_id,
                task["id"],
                exc,
            )
        return result

    def _cancel_shot_video_version(self, task: dict[str, Any]) -> None:
        """Mirror a cancellation into the version record created with the task."""

        snapshot = task.get("input_snapshot") or {}
        version_id = str(snapshot.get("version_id") or "")
        if not version_id:
            return
        self.repository.update_shot_version(
            version_id, status=GenerationStatus.CANCELLED
        )
