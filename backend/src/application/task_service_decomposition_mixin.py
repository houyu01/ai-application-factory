"""Initial short-drama expansion and decomposition task workflow."""

from __future__ import annotations

from typing import Any

from ..domain.models import GenerationStatus
from ..llm_service.planner import ScriptPlanner
from ..llm_service.planner_expansion_request_mixin import ExpansionCancelledError
from ..llm_service.planner_expansion_mixin import RetryableExpansionError


class TaskServiceDecompositionMixin:
    """Run the persisted bootstrap task from original script through shots.

    The durable ``script_decomposition`` task calls this mixin after project
    creation or worker recovery.  It owns the ordering boundary: persist a
    generated long-form screenplay first, then decompose that exact text.
    """

    def decompose_project(self, task_id: str, project_id: str) -> None:
        """Expand, persist, and decompose one project while updating task state."""

        try:
            self._raise_if_script_expansion_cancelled(task_id)
            project = self.get_project(project_id)
            self.repository.update_task_progress(task_id, error_message="")
            source_script = str(project.get("script") or "").strip()
            screenplay = self._script_for_decomposition(task_id, project, source_script)
            self._raise_if_script_expansion_cancelled(task_id)
            runtime = self._provider_options(project, "language")
            self.repository.update_task_progress(
                task_id, progress=65, stage="正在拆解扩写剧本"
            )
            if isinstance(self.planner, ScriptPlanner):
                plan = self.planner.plan(
                    screenplay,
                    options=runtime,
                    is_cancelled=lambda: self._script_expansion_cancelled(task_id),
                )
            else:
                plan = self.planner.plan(screenplay)
            if (
                isinstance(self.planner, ScriptPlanner)
                and self.planner._is_long_form_screenplay(screenplay, runtime)
            ):
                episodes = plan.get("episodes", [])
            else:
                episodes = ScriptPlanner._repair_shot_segments(
                    plan.get("episodes", []), screenplay, runtime
                )
            assets = plan.get("assets", [])
            self.repository.update_task_progress(
                task_id, progress=80, stage="正在整理分集、分镜和素材"
            )
            shots = self._flatten_shots(episodes, assets, project)
            self._raise_if_script_expansion_cancelled(task_id)
            self.repository.update_task_progress(
                task_id, progress=90, stage="正在保存分镜和素材"
            )
            self.repository.save_decomposition(project_id, episodes, shots, assets)
            self._raise_if_script_expansion_cancelled(task_id)
            self.repository.set_drama_status(project_id, GenerationStatus.SUCCEEDED)
            self.repository.update_task_status(
                task_id,
                GenerationStatus.SUCCEEDED,
                result={
                    "episodes": episodes,
                    "shots": shots,
                    "assets": assets,
                    "original_script_length": len(source_script),
                    "expanded_script_length": len(screenplay),
                },
            )
        except (RetryableExpansionError, ExpansionCancelledError):
            raise
        except Exception as exc:
            # Project deletion removes its task rows before the remote stream
            # stops.  A late preview/checkpoint callback must end quietly
            # instead of retrying a model request or reviving a deleted task.
            if (
                not self.repository.drama_exists(project_id)
                or self.repository.get_task(task_id) is None
            ):
                return
            try:
                self.repository.set_drama_status(project_id, GenerationStatus.FAILED)
                self.repository.update_task_status(
                    task_id, GenerationStatus.FAILED, error_message=str(exc)
                )
            except KeyError:
                # Deletion can race the existence check above in another DB
                # transaction; neither status needs to be restored.
                return

    def _script_for_decomposition(
        self, task_id: str, project: dict[str, Any], source_script: str
    ) -> str:
        """Return the persisted expanded screenplay or create it before planning."""

        if not source_script:
            raise ValueError("剧本内容不能为空")
        stored = self.repository.get_expanded_script(str(project["id"]))
        expanded = str(stored.get("expanded_script") or "").strip()
        runtime = self._provider_options(project, "language")
        minimum, maximum = self._expanded_script_limits(project)
        target = minimum
        is_long_form_ready = (
            not isinstance(self.planner, ScriptPlanner)
            or not self.planner._requires_long_form_expansion(runtime)
            or self.planner._is_long_form_screenplay(expanded, runtime)
        )
        if expanded and target <= len(expanded) <= maximum and is_long_form_ready:
            self.repository.update_task_progress(
                task_id, progress=60, stage="已读取已保存的扩写剧本"
            )
            return expanded

        expander = getattr(self.planner, "expand_script", None)
        if not callable(expander):
            self.repository.update_task_progress(
                task_id, progress=60, stage="未配置扩写器，按原始剧本拆解"
            )
            return source_script

        if isinstance(self.planner, ScriptPlanner):
            resumed_chars = min(len(expanded), target)
            resumed_progress = 5 + round(55 * resumed_chars / max(1, target))
            self.repository.update_task_progress(
                task_id,
                progress=resumed_progress,
                stage=f"正在扩写剧本（{resumed_chars:,}/{target:,} 字）",
            )
        else:
            self.repository.update_task_progress(task_id, progress=5, stage="正在扩写剧本")

        def report_progress(written: int, target: int) -> None:
            self._raise_if_script_expansion_cancelled(task_id)
            progress = 5 + round(55 * min(written, target) / max(1, target))
            self.repository.update_task_progress(
                task_id,
                progress=progress,
                stage=f"正在扩写剧本（{min(written, target):,}/{target:,} 字）",
            )

        def report_stage(stage: str) -> None:
            """Persist the current long-form expansion phase for the task UI."""

            self._raise_if_script_expansion_cancelled(task_id)
            self.repository.update_task_progress(task_id, stage=stage, error_message="")

        task_snapshot = dict((self.repository.get_task(task_id) or {}).get("input_snapshot") or {})
        story_bible = str(task_snapshot.get("story_bible") or "").strip()
        last_preview_length = 0

        def stream_preview(partial: str) -> None:
            nonlocal last_preview_length
            self._raise_if_script_expansion_cancelled(task_id)
            if len(partial) - last_preview_length < 120:
                return
            task_snapshot["expanded_script_preview"] = partial
            self.repository.update_task_input_snapshot(task_id, task_snapshot)
            last_preview_length = len(partial)

        def checkpoint(partial: str, written: int, target: int) -> None:
            nonlocal last_preview_length
            self._raise_if_script_expansion_cancelled(task_id)
            self.repository.update_expanded_script(str(project["id"]), partial)
            # Checkpoints make the full screenplay recoverable, while this
            # bounded task field remains the browser's live preview source.
            task_snapshot["expanded_script_preview"] = partial
            self.repository.update_task_input_snapshot(task_id, task_snapshot)
            last_preview_length = len(partial)
            report_progress(written, target)

        def checkpoint_story_bible(outline: str) -> None:
            """Persist the completed outline so a retry can begin screenplay writing."""

            self._raise_if_script_expansion_cancelled(task_id)
            task_snapshot["story_bible"] = outline
            self.repository.update_task_input_snapshot(task_id, task_snapshot)

        if isinstance(self.planner, ScriptPlanner):
            result = expander(
                source_script,
                options={**runtime, "enable_web_search": bool(project.get("enable_web_search", False))},
                existing_script=expanded,
                existing_outline=story_bible,
                checkpoint=checkpoint,
                outline_checkpoint=checkpoint_story_bible,
                stream=stream_preview,
                on_stage=report_stage,
                is_cancelled=lambda: self._script_expansion_cancelled(task_id),
            )
        else:
            result = expander(source_script)
        expanded = str(result or "").strip()
        if not expanded:
            self.repository.update_task_progress(
                task_id, progress=60, stage="未配置语言模型，按原始剧本拆解"
            )
            return source_script
        self._raise_if_script_expansion_cancelled(task_id)
        self.repository.update_expanded_script(str(project["id"]), expanded)
        self.repository.update_task_progress(
            task_id, progress=60, stage=f"扩写剧本已保存（{len(expanded):,} 字）"
        )
        return expanded

    def _script_expansion_cancelled(self, task_id: str) -> bool:
        """Read the durable cancel state without reviving a removed task."""

        task = self.repository.get_task(task_id)
        return task is None or task.get("status") == GenerationStatus.CANCELLED.value

    def _raise_if_script_expansion_cancelled(self, task_id: str) -> None:
        """Stop persistence callbacks after the creator cancels expansion."""

        if self._script_expansion_cancelled(task_id):
            raise ExpansionCancelledError("剧本扩写已取消")

    def _expanded_script_limits(self, project: dict[str, Any]) -> tuple[int, int]:
        """Resolve persisted screenplay limits for the project bootstrap task."""

        resolver = getattr(self.planner, "expansion_char_limits", None)
        if callable(resolver):
            return resolver(self._provider_options(project, "language"))
        minimum = int(getattr(self.planner, "EXPANDED_SCRIPT_TARGET_CHARS", 0) or 0)
        maximum = int(project.get("expanded_script_max_chars") or 10_000)
        return minimum, max(minimum, maximum)
