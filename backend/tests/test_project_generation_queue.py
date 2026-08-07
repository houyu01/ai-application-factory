"""Regression coverage for short-drama bootstrap queue metadata."""

from src.application.task_service import TaskService
from src.domain.models import GenerationStatus, ProjectCreate
from src.infrastructure.sqlite_repository import SQLiteRepository


def _project(name: str) -> ProjectCreate:
    """Build a minimal project whose bootstrap task remains available to claim."""

    return ProjectCreate(name=name, script=f"{name} 的短剧剧本内容足够长，可以进入后台生成。")


def test_project_list_marks_claimed_and_waiting_bootstrap_tasks(tmp_path) -> None:
    """Project cards expose FIFO positions and waiting state from durable tasks."""

    service = TaskService(SQLiteRepository(tmp_path / "project-queue.db"), object())
    first = service.create_project(_project("第一部"))
    second = service.create_project(_project("第二部"))

    assert first["queue_position"] == 1
    assert first["queue_state"] == "queued"
    assert second["queue_position"] == 2
    assert second["queue_state"] == "queued"

    claimed = service.repository.claim_next_runnable_task()

    assert claimed is not None
    assert claimed["id"] == first["task_id"]
    listed = {project["id"]: project for project in service.list_projects()}

    assert listed[first["id"]]["queue_position"] == 1
    assert listed[first["id"]]["queue_state"] == "processing"
    assert listed[second["id"]]["queue_position"] == 2
    assert listed[second["id"]]["queue_state"] == "queued"


def test_completed_project_is_removed_from_project_queue(tmp_path) -> None:
    """Only active bootstrap tasks receive list-card queue metadata."""

    service = TaskService(SQLiteRepository(tmp_path / "completed-project-queue.db"), object())
    project = service.create_project(_project("已完成"))
    service.repository.update_task_status(project["task_id"], status=GenerationStatus.SUCCEEDED)

    listed = service.list_projects()[0]

    assert "queue_position" not in listed
    assert "queue_state" not in listed


def test_progressing_script_task_remains_processing_after_its_lease_is_released(tmp_path) -> None:
    """A streamed expansion must not return to the waiting label between updates."""

    service = TaskService(SQLiteRepository(tmp_path / "project-progress-queue.db"), object())
    project = service.create_project(_project("流式扩写"))
    claimed = service.repository.claim_next_runnable_task()

    assert claimed is not None
    service.repository.update_task_progress(
        project["task_id"], progress=5, stage="正在扩写剧本（3,000/50,000 字）"
    )

    listed = {item["id"]: item for item in service.list_projects()}
    assert listed[project["id"]]["queue_state"] == "processing"


def test_new_script_generation_jumps_ahead_of_waiting_language_tasks(tmp_path) -> None:
    """A new short drama waits only for active workers, not older queued LLM work."""

    service = TaskService(SQLiteRepository(tmp_path / "priority-queue.db"), object())
    existing = service.create_project(_project("已有项目"))
    service.repository.update_task_status(existing["task_id"], GenerationStatus.SUCCEEDED)
    first_waiting = service.repository.create_task(existing["id"], "shot_prompt", "shot-1")
    second_waiting = service.repository.create_task(existing["id"], "shot_quality", "shot-2")
    for task in (first_waiting, second_waiting):
        service.repository.update_task_status(task["id"], GenerationStatus.GENERATING)

    language_tasks = {
        "script_decomposition", "script_expansion", "shot_prompt", "shot_quality",
    }
    active = service.repository.claim_next_runnable_task(task_types=language_tasks)
    priority_project = service.create_project(_project("优先短剧"))
    claimed = service.repository.claim_next_runnable_task(task_types=language_tasks)
    continuation, created = service.repository.create_active_task(
        existing["id"], "script_expansion"
    )
    continuation_claim = service.repository.claim_next_runnable_task(
        task_types=language_tasks
    )

    assert active is not None
    assert active["id"] in {first_waiting["id"], second_waiting["id"]}
    assert claimed is not None
    assert claimed["id"] == priority_project["task_id"]
    assert created is True
    assert continuation_claim is not None
    assert continuation_claim["id"] == continuation["id"]
