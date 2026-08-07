"""Regression coverage for editable short-drama project titles."""

from src.domain.models import ProjectCreate
from src.infrastructure.sqlite_repository import SQLiteRepository


def test_project_name_update_survives_repository_restart(tmp_path):
    repository = SQLiteRepository(tmp_path / "rename.db")
    project, _task = repository.create_drama_with_task(
        ProjectCreate(name="旧标题", script="这是一段用于创建短剧项目的基础剧本文本。")
    )

    updated = repository.update_project_name(project["id"], "  新标题  ")
    reopened = SQLiteRepository(tmp_path / "rename.db")

    assert updated["name"] == "新标题"
    assert reopened.get_drama(project["id"])["name"] == "新标题"
