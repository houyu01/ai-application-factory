"""Bounded project-detail reads for the short-drama editor."""

from __future__ import annotations


class TaskServiceDetailMixin:
    """Serve the editor-opening flow without exposing worker-sized payloads.

    The HTTP project-detail route calls this mixin when the user opens a
    drama. It owns the application boundary between the complete aggregate
    required by workers and the compact view required by the browser.
    """

    def get_editor_project(
        self, project_id: str, selected_shot_id: str | None = None
    ) -> dict:
        """Return the compact aggregate rendered by the project editor."""
        project = self.repository.get_drama_editor(project_id, selected_shot_id)
        if project is None:
            raise KeyError(f"Project not found: {project_id}")
        return project
