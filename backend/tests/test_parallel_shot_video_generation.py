"""Regression coverage for parallel video outputs from one drama shot."""

import pytest
from fastapi.testclient import TestClient

from src.api import video_generation_routes
from src.application.task_service import TaskService
from src.domain.models import GenerationStatus, ProjectCreate
from src.infrastructure.sqlite_repository import SQLiteRepository
from src.main import app


class OneShotPlanner:
    """Produce the smallest decomposed project used by video-task tests."""

    def plan(self, script: str) -> dict:
        """Return one editable shot with no image-reference prerequisites."""

        return {
            "episodes": [{"name": "第1集", "shots": [{"title": "开场", "original_text": script}]}],
            "assets": [],
        }


def prepared_service(tmp_path) -> tuple[TaskService, dict, dict]:
    """Create a project whose only shot has a valid saved video prompt."""

    service = TaskService(SQLiteRepository(tmp_path / "parallel-video.db"), OneShotPlanner())
    project = service.create_project(
        ProjectCreate(name="并行视频", script="林岩在旧宅门前发现一把铜钥匙。")
    )
    service.decompose_project(project["task_id"], project["id"])
    shot = service.get_project(project["id"])["shots"][0]
    service.repository.update_shot(project["id"], shot["id"], prompt="林岩在旧宅门前握紧铜钥匙。")
    return service, project, shot


def test_shot_video_batch_creates_independent_durable_versions(tmp_path) -> None:
    """A count of three must create three tasks and three pending versions together."""

    service, project, shot = prepared_service(tmp_path)

    tasks = service.enqueue_shot_videos(project["id"], shot["id"], 3)
    repeated = service.enqueue_shot_videos(project["id"], shot["id"], 3)

    assert len(tasks) == 3
    assert len({task["id"] for task in tasks}) == 3
    assert {task["status"] for task in tasks} == {GenerationStatus.GENERATING.value}
    assert [task["id"] for task in repeated] == [task["id"] for task in tasks]
    assert len(service.repository.list_shot_versions(project["id"], shot["id"])) == 3


def test_shot_video_batch_rejects_counts_outside_the_editor_range(tmp_path) -> None:
    """The service must protect the UI's one-to-three selection contract."""

    service, project, shot = prepared_service(tmp_path)

    with pytest.raises(ValueError, match="1 到 3"):
        service.enqueue_shot_videos(project["id"], shot["id"], 4)


def test_cancelling_a_shot_stops_every_parallel_video_task(tmp_path) -> None:
    """The shot action must not leave sibling batch tasks running in the provider."""

    service, project, shot = prepared_service(tmp_path)
    tasks = service.enqueue_shot_videos(project["id"], shot["id"], 3)

    cancelled = service.cancel_shot_video(project["id"], shot["id"])

    assert cancelled["cancelled_count"] == 3
    assert {
        service.repository.get_task(task["id"])["status"] for task in tasks
    } == {GenerationStatus.CANCELLED.value}
    assert {
        version["status"]
        for version in service.repository.list_shot_versions(project["id"], shot["id"])
    } == {GenerationStatus.CANCELLED.value}


def test_parallel_video_route_returns_every_created_task(monkeypatch) -> None:
    """The button's plural endpoint must expose all batch task IDs to the browser."""

    monkeypatch.setattr(
        video_generation_routes.task_service,
        "enqueue_shot_videos",
        lambda project_id, shot_id, count, public_media_base_url=None: [
            {
                "id": f"video-{index}", "type": "shot_video", "status": "生成中",
                "project_id": project_id, "resource_id": shot_id,
                "created_at": "2026-08-07T00:00:00Z", "progress": 0, "stage": "",
                "input_snapshot": {"version_id": f"version-{index}"},
            }
            for index in range(1, count + 1)
        ],
    )

    response = TestClient(app).post(
        "/api/projects/project-1/shots/shot-1/videos", json={"count": 3}
    )

    assert response.status_code == 202
    assert response.json()["requested_count"] == 3
    assert [task["id"] for task in response.json()["tasks"]] == ["video-1", "video-2", "video-3"]
