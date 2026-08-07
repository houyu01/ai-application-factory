"""Cancellation flow for durable short-drama asset image generations."""

from __future__ import annotations

from typing import Any

from ..domain.models import GenerationStatus


class TaskServiceAssetCancellationMixin:
    """Stop asset-panel image work without affecting another asset category.

    The character, scene, and prop drawers call this mixin from their bulk
    cancellation action. It owns the durable boundary for a selected category:
    cancel its batch coordinator first, then its current base-image and variant
    tasks so the image worker cannot schedule another item from that batch.
    """

    ASSET_IMAGE_TYPES = {"character", "scene", "prop"}

    def cancel_asset_image_tasks(
        self, project_id: str, asset_type: str
    ) -> dict[str, Any]:
        """Cancel all active base and variant image tasks for one asset drawer tab."""

        project = self.get_project(project_id)
        normalized_type = str(asset_type or "").strip()
        if normalized_type not in self.ASSET_IMAGE_TYPES:
            raise ValueError("只支持取消角色、场景或道具的图片任务")
        assets = [
            asset for asset in project.get("assets", [])
            if asset.get("type") == normalized_type
        ]
        asset_ids = {str(asset.get("id")) for asset in assets if asset.get("id")}
        variant_parents = {
            str(variant.get("id")): str(asset["id"])
            for asset in assets
            for variant in asset.get("variants", [])
            if variant.get("id") and asset.get("id")
        }
        batch_tasks = [
            task for task in self.repository.list_active_tasks(project_id, "asset_image_batch")
            if str(task.get("resource_id") or "") == normalized_type
        ]
        image_tasks = [
            task for task in self.repository.list_active_tasks(project_id, "asset_image")
            if str(task.get("resource_id") or "") in asset_ids
        ]
        variant_tasks = [
            task for task in self.repository.list_active_tasks(project_id, "asset_variant_image")
            if str(task.get("resource_id") or "") in variant_parents
        ]
        cancelled_tasks: list[dict[str, Any]] = []
        for task in batch_tasks:
            cancelled = self._cancel_asset_image_task(task, normalized_type, variant_parents)
            if cancelled is not None:
                cancelled_tasks.append(cancelled)
        for task in [*image_tasks, *variant_tasks]:
            cancelled = self._cancel_asset_image_task(task, normalized_type, variant_parents)
            if cancelled is not None:
                cancelled_tasks.append(cancelled)
        return {
            "project_id": project_id,
            "asset_type": normalized_type,
            "cancelled_count": len(cancelled_tasks),
            "cancelled_tasks": cancelled_tasks,
        }

    def _cancel_asset_image_task(
        self,
        task: dict[str, Any],
        asset_type: str,
        variant_parents: dict[str, str],
    ) -> dict[str, Any] | None:
        """Cancel one image task and reflect its terminal state on its asset card."""

        try:
            cancelled = self.repository.cancel_task(
                str(task["id"]), stage=f"{self._asset_type_label(asset_type)}图片生成已取消"
            )
        except ValueError:
            return None
        task_type = str(task.get("type") or "")
        resource_id = str(task.get("resource_id") or "")
        if task_type == "asset_image":
            self.repository.update_asset_status(resource_id, GenerationStatus.CANCELLED)
        elif task_type == "asset_variant_image":
            parent_id = variant_parents.get(resource_id)
            if parent_id:
                self.repository.update_asset_variant_status(
                    str(task["drama_id"]), parent_id, resource_id, GenerationStatus.CANCELLED
                )
        return cancelled

    def _asset_image_task_cancelled(self, task_id: str) -> bool:
        """Check cancellation before a synchronous image provider commits a result."""

        task = self.repository.get_task(task_id)
        return task is None or task.get("status") == GenerationStatus.CANCELLED.value

    @staticmethod
    def _asset_type_label(asset_type: str) -> str:
        """Return the visible label used in a task's cancellation stage."""

        return {"character": "角色", "scene": "场景", "prop": "道具"}[asset_type]
