"""Durable coordinator for bounded batches of drama asset image tasks."""

from __future__ import annotations

from typing import Any

from ..domain.models import GenerationStatus


class TaskServiceAssetBatchMixin:
    """Run asset-drawer and selected-reference images in ordered batches of five.

    The asset drawer starts this coordinator instead of creating every image
    task at once.  It persists the ordered work list and the active child task
    ids, so restarting the browser or worker continues from the same batch.
    """

    ASSET_IMAGE_BATCH_SIZE = 5

    def enqueue_asset_image_batch(
        self, project_id: str, asset_ids: list[str]
    ) -> dict[str, Any]:
        """Create one idempotent batch task for the selected asset drawer tab."""

        project = self.get_project(project_id)
        selected_ids = list(dict.fromkeys(str(asset_id) for asset_id in asset_ids))
        if not selected_ids:
            raise ValueError("请至少选择一个素材")
        assets = {
            str(asset.get("id")): asset for asset in project.get("assets", [])
        }
        selected = [assets.get(asset_id) for asset_id in selected_ids]
        if any(asset is None for asset in selected):
            raise KeyError("素材不存在或不属于当前项目")
        asset_types = {str(asset.get("type") or "") for asset in selected if asset}
        if len(asset_types) != 1 or not asset_types <= {"character", "scene", "prop"}:
            raise ValueError("生成全部图片时只能选择同一类素材")

        jobs: list[dict[str, str]] = []
        for asset in selected:
            if asset is None:
                continue
            asset_id = str(asset["id"])
            jobs.append({"type": "asset_image", "asset_id": asset_id})
            jobs.extend(
                {
                    "type": "asset_variant_image",
                    "asset_id": asset_id,
                    "variant_id": str(variant["id"]),
                }
                for variant in asset.get("variants", [])
                if variant.get("id")
            )
        asset_type = next(iter(asset_types))
        task, created = self.repository.create_active_task(
            project_id,
            "asset_image_batch",
            asset_type,
            input_snapshot={
                "project_id": project_id,
                "asset_type": asset_type,
                "jobs": jobs,
                "batch_size": self.ASSET_IMAGE_BATCH_SIZE,
                "next_index": 0,
                "active_task_ids": [],
                "completed_count": 0,
                "failed_count": 0,
                "cancelled_count": 0,
                "type": "asset_image_batch",
            },
        )
        return {**task, "_reused": not created}

    def enqueue_missing_shot_reference_images(
        self, project_id: str, shot_id: str
    ) -> dict[str, Any]:
        """Create a durable batch for ungenerated assets selected by one video shot.

        The video editor invokes this flow from its unavailable-reference warning.
        It owns the boundary between the shot's persisted rich references and
        reusable character, scene, and prop image tasks, leaving ready images
        unchanged and reusing any image task already in progress.
        """

        project = self.get_project(project_id)
        shot = self.repository.get_shot(project_id, shot_id)
        if shot is None:
            raise KeyError(f"Shot not found: {shot_id}")
        reference_ids = self._shot_reference_asset_ids(shot)
        assets = {
            str(asset.get("id")): asset
            for asset in project.get("assets", [])
            if asset.get("id")
        }
        selected = [assets.get(asset_id) for asset_id in reference_ids]
        pending_assets = [
            asset
            for asset in selected
            if asset
            and asset.get("type") in {"character", "scene", "prop"}
            and not str(asset.get("image_url") or "").strip()
        ]
        if not pending_assets:
            raise ValueError("当前已选参考图均已生成，或没有可一键生成的素材")

        asset_ids = [str(asset["id"]) for asset in pending_assets]
        task, created = self.repository.create_active_task(
            project_id,
            "shot_reference_image_batch",
            shot_id,
            input_snapshot={
                "project_id": project_id,
                "shot_id": shot_id,
                "reference_asset_ids": reference_ids,
                "asset_ids": asset_ids,
                "jobs": [{"type": "asset_image", "asset_id": asset_id} for asset_id in asset_ids],
                "batch_size": self.ASSET_IMAGE_BATCH_SIZE,
                "next_index": 0,
                "active_task_ids": [],
                "completed_count": 0,
                "failed_count": 0,
                "cancelled_count": 0,
                "type": "shot_reference_image_batch",
            },
        )
        return {**task, "_reused": not created}

    @staticmethod
    def _shot_reference_asset_ids(shot: dict[str, Any]) -> list[str]:
        """Preserve rich-prompt reference order, with legacy ids as a fallback."""

        prompt_rich = shot.get("prompt_rich") or []
        reference_ids = [
            str(node.get("asset_id"))
            for node in prompt_rich
            if isinstance(node, dict)
            and node.get("type") == "reference"
            and node.get("asset_id")
        ]
        if not reference_ids:
            reference_ids = [
                str(asset_id)
                for asset_id in shot.get("reference_asset_ids") or []
                if asset_id
            ]
        return list(dict.fromkeys(reference_ids))

    def run_asset_image_batch(self, task: dict[str, Any]) -> None:
        """Advance one persisted batch after its current five jobs reach a terminal state."""

        task_id = str(task["id"])
        if self._asset_image_task_cancelled(task_id):
            return
        project_id = str(task["drama_id"])
        snapshot = dict(task.get("input_snapshot") or {})
        jobs = [item for item in snapshot.get("jobs", []) if isinstance(item, dict)]
        active_task_ids = [
            str(item) for item in snapshot.get("active_task_ids", []) if str(item)
        ]
        if not jobs:
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message="素材批次没有可执行的图片任务"
            )
            return

        active_tasks = [self.repository.get_task(child_id) for child_id in active_task_ids]
        if any(
            child is not None and child.get("status") == GenerationStatus.GENERATING.value
            for child in active_tasks
        ):
            self.repository.update_task_progress(
                task_id,
                progress=self._asset_batch_progress(snapshot, len(jobs)),
                stage=self._asset_batch_stage(snapshot, len(jobs)),
                next_poll_at=self._asset_batch_next_poll(),
            )
            return

        if active_task_ids:
            failed_count = sum(
                child is None or child.get("status") == GenerationStatus.FAILED.value
                for child in active_tasks
            )
            cancelled_count = sum(
                child is not None and child.get("status") == GenerationStatus.CANCELLED.value
                for child in active_tasks
            )
            snapshot["completed_count"] = int(snapshot.get("completed_count") or 0) + len(active_task_ids)
            snapshot["failed_count"] = int(snapshot.get("failed_count") or 0) + failed_count
            snapshot["cancelled_count"] = int(snapshot.get("cancelled_count") or 0) + cancelled_count
            snapshot["active_task_ids"] = []

        next_index = int(snapshot.get("next_index") or 0)
        if next_index >= len(jobs):
            completed = int(snapshot.get("completed_count") or 0)
            failed = int(snapshot.get("failed_count") or 0)
            cancelled = int(snapshot.get("cancelled_count") or 0)
            self.repository.update_task_input_snapshot(task_id, snapshot)
            self.repository.update_task_status(
                task_id,
                GenerationStatus.SUCCEEDED,
                result={
                    "total_count": len(jobs),
                    "completed_count": completed,
                    "failed_count": failed,
                    "cancelled_count": cancelled,
                },
            )
            return

        batch_size = min(
            self.ASSET_IMAGE_BATCH_SIZE,
            max(1, int(snapshot.get("batch_size") or self.ASSET_IMAGE_BATCH_SIZE)),
        )
        batch_jobs = jobs[next_index : next_index + batch_size]
        child_task_ids = [
            self._enqueue_asset_batch_job(project_id, job)["id"] for job in batch_jobs
        ]
        if self._asset_image_task_cancelled(task_id):
            self.cancel_asset_image_tasks(
                project_id, str(snapshot.get("asset_type") or "")
            )
            return
        snapshot["next_index"] = next_index + len(batch_jobs)
        snapshot["active_task_ids"] = child_task_ids
        self.repository.update_task_input_snapshot(task_id, snapshot)
        self.repository.update_task_progress(
            task_id,
            progress=self._asset_batch_progress(snapshot, len(jobs)),
            stage=self._asset_batch_stage(snapshot, len(jobs)),
            next_poll_at=self._asset_batch_next_poll(),
        )

    def _enqueue_asset_batch_job(
        self, project_id: str, job: dict[str, Any]
    ) -> dict[str, Any]:
        """Create or reuse one child task belonging to the current batch."""

        asset_id = str(job.get("asset_id") or "")
        if job.get("type") == "asset_variant_image":
            return self.enqueue_asset_variant_image(
                project_id, asset_id, str(job.get("variant_id") or "")
            )
        return self.enqueue("asset_image", project_id, asset_id)

    @staticmethod
    def _asset_batch_progress(snapshot: dict[str, Any], total: int) -> int:
        """Return completed-job progress without treating queued work as finished."""

        if total <= 0:
            return 0
        return min(99, int(int(snapshot.get("completed_count") or 0) * 100 / total))

    @staticmethod
    def _asset_batch_next_poll() -> str:
        """Reuse the repository's ISO scheduling contract without importing worker helpers."""

        from datetime import datetime, timedelta, timezone

        return (datetime.now(timezone.utc) + timedelta(seconds=1)).isoformat()

    @staticmethod
    def _asset_batch_stage(snapshot: dict[str, Any], total: int) -> str:
        """Describe the current five-image window for task polling and diagnostics."""

        completed = int(snapshot.get("completed_count") or 0)
        next_index = int(snapshot.get("next_index") or 0)
        current_start = completed + 1
        current_end = min(next_index, total)
        return f"正在生成第 {current_start}-{current_end} / {total} 张素材图片"
