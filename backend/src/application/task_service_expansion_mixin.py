"""Durable continuation tasks for screenplay text shown in the project dialog."""

from __future__ import annotations

from typing import Any

from ..domain.models import GenerationStatus
from ..llm_service.planner_expansion_mixin import RetryableExpansionError
from ..llm_service.planner_expansion_request_mixin import ExpansionCancelledError


class TaskServiceExpansionMixin:
    """Coordinate dialog-triggered screenplay continuation without rebuilding shots.

    The screenplay dialog invokes this flow after initial script decomposition
    has succeeded. It owns durable task state and screenplay checkpoints; the
    worker owns provider execution, while the existing storyboard stays intact.
    """

    SCRIPT_EXPANSION_TASK_TYPES = {"script_decomposition", "script_expansion"}

    def get_expanded_script(self, project_id: str) -> dict[str, Any]:
        """Load dialog text and the latest initial or continued expansion state."""

        screenplay = self.repository.get_expanded_script(project_id)
        tasks = self.repository.list_task_statuses(project_id)
        expansion_tasks = [
            task for task in tasks
            if task.get("type") in self.SCRIPT_EXPANSION_TASK_TYPES
        ]
        latest_task = expansion_tasks[-1] if expansion_tasks else None
        active_task = latest_task if latest_task and latest_task.get("status") == GenerationStatus.GENERATING.value else None
        stage = str((latest_task or {}).get("stage") or "")
        screenplay["expanded_script_generating"] = bool(active_task)
        screenplay["expanded_script_cancellable"] = bool(active_task) and not (
            active_task and active_task.get("type") == "script_decomposition" and "拆解" in stage
        )
        screenplay["expanded_script_task_status"] = (latest_task or {}).get("status")
        screenplay["expanded_script_error_message"] = (latest_task or {}).get("error_message")
        screenplay["expanded_script_stage"] = stage
        snapshot = (latest_task or {}).get("input_snapshot") or {}
        preview = str(snapshot.get("expanded_script_preview") or "")
        screenplay["expanded_script_preview"] = preview
        screenplay["expanded_script_length"] = max(
            int(screenplay.get("expanded_script_length") or 0), len(preview)
        )
        return screenplay

    def continue_expanded_script(self, project_id: str) -> dict[str, Any]:
        """Enqueue one append-only LLM continuation from the saved screenplay.

        The dialog calls this on ``继续扩写``. Repeated clicks return the
        running task, preserving idempotent enqueue behavior until it finishes.
        """

        project = self.get_project(project_id)
        active_task = self.active_expanded_script_task(project_id)
        if active_task:
            return active_task
        stored = self.repository.get_expanded_script(project_id)
        existing = str(stored.get("expanded_script") or "").strip()
        if not existing:
            raise ValueError("请先完成首次剧本扩写后再继续扩写")
        maximum = self._expanded_script_maximum(project)
        if maximum - len(existing) - 2 < 200:
            raise ValueError(f"扩写剧本已达到 {maximum} 字上限，无法继续扩写")
        task, _ = self.repository.create_active_task(
            project_id,
            "script_expansion",
            input_snapshot={
                "continuation_base_length": len(existing),
                "expanded_script_preview": existing,
                "story_bible": self._latest_expansion_story_bible(project_id),
            },
        )
        return task

    def cancel_expanded_script(self, project_id: str) -> dict[str, Any]:
        """Cancel a running expansion or confirm that its failed task has stopped.

        The screenplay dialog can submit a click while its last status refresh
        is still showing a cancellable task.  If the worker has already saved
        a failure by then, returning that failed task confirms that no worker
        or provider call remains to cancel while preserving the failure reason.
        """

        self.get_project(project_id)
        latest_task = self.latest_expanded_script_task(project_id)
        if latest_task is None:
            raise ValueError("未找到可取消的剧本扩写任务")
        if latest_task.get("status") == GenerationStatus.FAILED.value:
            return latest_task
        if latest_task.get("status") == GenerationStatus.CANCELLED.value:
            return latest_task
        if latest_task.get("status") != GenerationStatus.GENERATING.value:
            raise ValueError("剧本扩写已完成，无法取消")
        task = self.repository.cancel_task(
            str(latest_task["id"]), stage="剧本扩写已取消"
        )
        if latest_task.get("type") == "script_decomposition":
            self.repository.set_drama_status(project_id, GenerationStatus.CANCELLED)
        return task

    def cancel_script_decomposition(self, project_id: str) -> dict[str, Any]:
        """Keep the original cancellation service name compatible with its route."""

        return self.cancel_expanded_script(project_id)

    def active_expanded_script_task(self, project_id: str) -> dict[str, Any] | None:
        """Return the latest running task that owns the screenplay text."""

        latest_task = self.latest_expanded_script_task(project_id)
        if latest_task and latest_task.get("status") == GenerationStatus.GENERATING.value:
            return latest_task
        return None

    def latest_expanded_script_task(self, project_id: str) -> dict[str, Any] | None:
        """Return the newest task whose state controls the screenplay dialog."""

        tasks = self.repository.list_task_statuses(project_id)
        return next(
            (
                task for task in reversed(tasks)
                if task.get("type") in self.SCRIPT_EXPANSION_TASK_TYPES
            ),
            None,
        )

    def run_expanded_script_continuation(self, task_id: str, project_id: str) -> None:
        """Resume one append-only screenplay task claimed by the durable worker."""

        try:
            self._raise_if_script_expansion_cancelled(task_id)
            project = self.get_project(project_id)
            task = self.repository.get_task(task_id)
            if task is None:
                return
            snapshot = dict(task.get("input_snapshot") or {})
            stored = self.repository.get_expanded_script(project_id)
            existing = str(stored.get("expanded_script") or "").strip()
            base_length = int(snapshot.get("continuation_base_length") or len(existing))
            if snapshot.get("continuation_complete") or len(existing) > base_length:
                self._finish_expanded_script_continuation(task_id, project_id, existing)
                return
            if not existing:
                raise ValueError("尚无可继续扩写的剧本内容")
            self.repository.update_task_progress(
                task_id, progress=5, stage="正在继续扩写剧本", error_message=""
            )
            last_preview_length = len(str(snapshot.get("expanded_script_preview") or ""))

            def report_stage(stage: str) -> None:
                self._raise_if_script_expansion_cancelled(task_id)
                self.repository.update_task_progress(task_id, stage=stage, error_message="")

            def stream_preview(partial: str) -> None:
                nonlocal last_preview_length
                self._raise_if_script_expansion_cancelled(task_id)
                if len(partial) - last_preview_length < 120:
                    return
                snapshot["expanded_script_preview"] = partial
                self.repository.update_task_input_snapshot(task_id, snapshot)
                last_preview_length = len(partial)

            def checkpoint(partial: str, written: int, target: int) -> None:
                self._raise_if_script_expansion_cancelled(task_id)
                self.repository.update_expanded_script(project_id, partial)
                snapshot["expanded_script_preview"] = partial
                snapshot["continuation_complete"] = True
                self.repository.update_task_input_snapshot(task_id, snapshot)
                progress = 5 + round(90 * max(0, written - base_length) / max(1, target - base_length))
                self.repository.update_task_progress(
                    task_id, progress=min(95, progress), stage="继续扩写内容已保存"
                )

            def checkpoint_story_bible(outline: str) -> None:
                self._raise_if_script_expansion_cancelled(task_id)
                snapshot["story_bible"] = outline
                self.repository.update_task_input_snapshot(task_id, snapshot)

            continuation = getattr(self.planner, "continue_expanded_script", None)
            if not callable(continuation):
                raise RuntimeError("当前剧本扩写器不支持继续扩写")
            result = continuation(
                str(project.get("script") or ""),
                existing,
                options={
                    **self._provider_options(project, "language"),
                    "enable_web_search": bool(project.get("enable_web_search", False)),
                },
                existing_outline=str(snapshot.get("story_bible") or ""),
                checkpoint=checkpoint,
                outline_checkpoint=checkpoint_story_bible,
                stream=stream_preview,
                on_stage=report_stage,
                is_cancelled=lambda: self._script_expansion_cancelled(task_id),
            )
            continued = str(result or "").strip()
            if not continued:
                raise RuntimeError("继续扩写没有返回有效内容")
            if not snapshot.get("continuation_complete"):
                checkpoint(continued, len(continued), len(continued))
            self._raise_if_script_expansion_cancelled(task_id)
            self._finish_expanded_script_continuation(task_id, project_id, continued)
        except (RetryableExpansionError, ExpansionCancelledError):
            raise
        except Exception as exc:
            current = self.repository.get_task(task_id)
            if current is None or current.get("status") == GenerationStatus.CANCELLED.value:
                return
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message=str(exc)
            )

    def _finish_expanded_script_continuation(
        self, task_id: str, project_id: str, screenplay: str
    ) -> None:
        """Mark a checkpointed continuation complete without changing shots."""

        self.repository.update_task_status(
            task_id,
            GenerationStatus.SUCCEEDED,
            result={
                "original_script_length": len(
                    str(self.repository.get_expanded_script(project_id).get("script") or "")
                ),
                "expanded_script_length": len(screenplay),
            },
        )

    def _expanded_script_maximum(self, project: dict[str, Any]) -> int:
        """Resolve the same persisted maximum enforced by the planner request."""

        resolver = getattr(self.planner, "expansion_char_limits", None)
        if callable(resolver):
            _, maximum = resolver(self._provider_options(project, "language"))
            return maximum
        return int(project.get("expanded_script_max_chars") or 100_000)

    def _latest_expansion_story_bible(self, project_id: str) -> str:
        """Reuse the latest saved outline so follow-up prose keeps continuity."""

        tasks = self.repository.list_task_statuses(project_id)
        for task in reversed(tasks):
            if task.get("type") not in self.SCRIPT_EXPANSION_TASK_TYPES:
                continue
            outline = str((task.get("input_snapshot") or {}).get("story_bible") or "").strip()
            if outline:
                return outline
        return ""
