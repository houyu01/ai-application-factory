"""Regression coverage for the bounded short-drama editor payload."""

import json

from src.application.task_service import TaskService
from src.domain.models import ProjectCreate
from src.infrastructure.sqlite_repository import SQLiteRepository
from src.infrastructure.sqlite_repository_mapping import DramaRepositoryMappingMixin


def test_editor_detail_excludes_project_script_and_unselected_shot_bodies(tmp_path):
    """Opening an editor must not return every generated field in the project."""

    repository = SQLiteRepository(tmp_path / "editor-detail.db")
    project, _ = repository.create_drama_with_task(
        ProjectCreate(name="项目", script="原始剧本内容足够用于创建项目。")
    )
    long_text = "剧情正文" * 500
    repository.save_decomposition(
        project["id"],
        [{"id": "episode-1", "title": "第1集"}],
        [
            {"id": "first", "title": "首个分镜", "original_text": long_text, "prompt": long_text},
            {"id": "second", "title": "第二个分镜", "original_text": long_text, "prompt": long_text},
        ],
        [],
    )

    full = repository.get_drama(project["id"])
    selected_id = full["shots"][1]["id"]
    detail = repository.get_drama_editor(project["id"], selected_id)

    assert detail["script"] == ""
    assert detail["shots"][1]["prompt"] == long_text
    assert detail["shots"][0]["prompt"] == ""
    assert detail["shots"][0]["original_text"].endswith("…")


def test_editor_task_preview_is_bounded_to_avoid_returning_full_screenplay():
    """A 50,000-character checkpoint must not inflate every editor refresh."""

    full_preview = "长篇剧本" * 10_000
    task = DramaRepositoryMappingMixin._detail_task_from_row(
        {
            "id": "task-1",
            "drama_id": "drama-1",
            "input_snapshot_json": json.dumps(
                {"expanded_script_preview": full_preview}, ensure_ascii=False
            ),
            "output_result_json": json.dumps({"screenplay": full_preview}, ensure_ascii=False),
        }
    )

    preview = task["input_snapshot"]["expanded_script_preview"]
    assert len(preview) <= DramaRepositoryMappingMixin.DETAIL_EXPANDED_PREVIEW_LIMIT + 32
    assert "已省略" in preview
    assert task["result"] is None


def test_editor_detail_includes_bootstrap_queue_state(tmp_path):
    """The initial progress banner needs the durable queue state after refresh."""

    service = TaskService(SQLiteRepository(tmp_path / "editor-queue.db"), object())
    project = service.create_project(ProjectCreate(name="排队项目", script="可排队处理的短剧内容。"))

    assert service.get_editor_project(project["id"])["queue_state"] == "queued"

    assert service.repository.claim_next_runnable_task() is not None

    assert service.get_editor_project(project["id"])["queue_state"] == "processing"
