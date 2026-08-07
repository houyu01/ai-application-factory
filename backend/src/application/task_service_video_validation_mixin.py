"""Preflight validation for durable short-drama video generation."""

from __future__ import annotations

from typing import Any


class TaskServiceVideoValidationMixin:
    """Guard the editor's Generate Video action before it creates a durable task.

    The short-drama editor calls this boundary through ``TaskService.enqueue``.
    It owns input-readiness rules that must also hold for API callers outside
    the browser: a shot needs usable prompt text and every referenced image
    must be ready for the configured video provider.
    """

    def _video_generation_preflight_issues(
        self,
        project: dict[str, Any],
        shot: dict[str, Any],
        public_media_base_url: str | None = None,
    ) -> list[str]:
        """Return all editor-visible blockers without creating a video task."""

        issues: list[str] = []
        if not str(shot.get("prompt") or "").strip():
            issues.append("请先生成或保存分镜提示词")
        issues.extend(
            self._missing_video_references(
                project, shot, public_media_base_url
            )
        )
        return issues
