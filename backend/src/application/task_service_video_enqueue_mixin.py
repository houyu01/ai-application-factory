"""Batch creation flow for parallel short-drama video generations."""

from __future__ import annotations

from typing import Any

from ..domain.models import GenerationStatus


class TaskServiceVideoEnqueueMixin:
    """Create the durable video runs requested by a shot editor action.

    The short-drama editor calls this flow when a creator selects one to three
    outputs and clicks Generate Video. It owns batch-level idempotency and
    keeps every requested output as an independent task and shot version.
    """

    def enqueue_shot_videos(
        self,
        project_id: str,
        shot_id: str,
        count: int,
        public_media_base_url: str | None = None,
    ) -> list[dict[str, Any]]:
        """Create one to three video tasks together, reusing an active batch.

        Each row remains independently pollable and cancellable. The local
        claim lock prevents a repeated click from adding a second parallel
        batch before the first set of durable rows has been recorded.
        """

        if count < 1 or count > 3:
            raise ValueError("一次生成视频的数量必须为 1 到 3")
        with self.repository.database.task_claim_lock:
            active_tasks = [
                task for task in self.repository.list_active_tasks(project_id, "shot_video")
                if task.get("resource_id") == shot_id
            ]
            if active_tasks:
                return [{**task, "_reused": True} for task in active_tasks]
            return [
                self.enqueue(
                    "shot_video",
                    project_id,
                    shot_id,
                    public_media_base_url=public_media_base_url,
                    allow_parallel=True,
                )
                for _ in range(count)
            ]

    def sync_shot_video_status(
        self,
        project_id: str,
        shot_id: str,
        completed_status: GenerationStatus,
    ) -> None:
        """Keep a shot generating until every parallel video run is terminal."""

        has_active_task = any(
            task.get("resource_id") == shot_id
            for task in self.repository.list_active_tasks(project_id, "shot_video")
        )
        self.repository.update_shot(
            project_id,
            shot_id,
            status=GenerationStatus.GENERATING if has_active_task else completed_status,
        )
