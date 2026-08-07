"""Regression coverage for removing a short-drama preview-history video."""

from src.api import video_history_routes
from fastapi.testclient import TestClient
from src.main import app
from src.application.task_service import TaskService
from src.domain.models import GenerationStatus, ProjectCreate
from src.infrastructure.media_store import media_store
from src.infrastructure.sqlite_repository import SQLiteRepository


class OneShotPlanner:
    """Build the smallest decomposition needed to exercise one video history."""

    def plan(self, script: str) -> dict:
        return {
            "episodes": [{"name": "第1集", "shots": [{"title": "开场", "original_text": script}]}],
            "assets": [],
        }


def test_delete_history_video_removes_preview_and_managed_media(tmp_path, monkeypatch) -> None:
    """Deleting a history card clears its records, version link, and local file."""

    media_root = tmp_path / "media"
    media_root.mkdir()
    monkeypatch.setattr(media_store, "root", media_root)
    media_store.configure({"provider": "local"})
    service = TaskService(SQLiteRepository(tmp_path / "drama.db"), OneShotPlanner())
    project = service.create_project(ProjectCreate(name="历史视频", script="林岩在旧宅门前发现一把铜钥匙。"))
    service.decompose_project(project["task_id"], project["id"])
    shot = service.get_project(project["id"])["shots"][0]
    media_url = media_store.save(b"video", ".mp4")
    service.repository.add_historical_video(
        project["id"], shot["id"],
        {"id": "video-task-1", "task_id": "video-task-1", "url": media_url},
    )

    result = service.delete_shot_historical_video(project["id"], shot["id"], "video-task-1")

    saved = service.get_project(project["id"])
    assert result["status"] == "deleted"
    assert result["media_deleted"] == 1
    assert saved["shots"][0]["historical_videos"] == []
    assert saved["historical_videos"] == []
    assert media_store.path_for(media_url.rsplit("/", 1)[-1]) is None


def test_delete_history_video_removes_failed_version_and_task(tmp_path) -> None:
    """A failed video run disappears from the durable shot history and queue."""

    service = TaskService(SQLiteRepository(tmp_path / "drama.db"), OneShotPlanner())
    project = service.create_project(ProjectCreate(name="失败历史", script="林岩进入旧宅调查线索。"))
    service.decompose_project(project["task_id"], project["id"])
    shot = service.get_project(project["id"])["shots"][0]
    version = service.repository.create_shot_version(
        project["id"], shot["id"], status=GenerationStatus.GENERATING,
    )
    task = service.repository.create_task(project["id"], "shot_video", shot["id"])
    service.repository.update_shot_version(
        version["id"], task_id=task["id"], status=GenerationStatus.FAILED,
        error_message="模型不支持当前视频请求。",
    )
    service.repository.update_task_status(
        task["id"], GenerationStatus.FAILED, error_message="模型不支持当前视频请求。",
    )

    result = service.delete_shot_historical_video(project["id"], shot["id"], version["id"])

    saved_shot = service.get_project(project["id"])["shots"][0]
    assert result["status"] == "deleted"
    assert service.repository.get_task(task["id"]) is None
    assert saved_shot["versions"] == []


def test_delete_history_video_route_delegates_to_service(monkeypatch) -> None:
    """The history-card DELETE route keeps its service boundary intact."""

    expected = {"id": "video-1", "status": "deleted"}
    monkeypatch.setattr(
        video_history_routes.task_service,
        "delete_shot_historical_video",
        lambda project_id, shot_id, video_id: {**expected, "id": video_id},
    )

    assert video_history_routes.delete_project_shot_video("project-1", "shot-1", "video-1") == expected


def test_cancel_video_route_delegates_to_the_video_task_service(monkeypatch) -> None:
    """The split-button menu must reach the dedicated task cancellation flow."""

    expected = {"id": "task-1", "status": GenerationStatus.CANCELLED.value}
    monkeypatch.setattr(
        video_history_routes.task_service,
        "cancel_shot_video",
        lambda project_id, shot_id: {**expected, "project_id": project_id, "resource_id": shot_id},
    )

    assert video_history_routes.cancel_project_shot_video("project-1", "shot-1") == {
        **expected,
        "project_id": "project-1",
        "resource_id": "shot-1",
    }


def test_cancel_video_route_is_exposed_over_http(monkeypatch) -> None:
    """The editor's POST endpoint must be registered on the shared API router."""

    monkeypatch.setattr(
        video_history_routes.task_service,
        "cancel_shot_video",
        lambda project_id, shot_id: {
            "id": "task-2",
            "status": GenerationStatus.CANCELLED.value,
            "project_id": project_id,
            "resource_id": shot_id,
        },
    )

    response = TestClient(app).post("/api/projects/project-2/shots/shot-2/video/cancel")

    assert response.status_code == 202
    assert response.json()["status"] == GenerationStatus.CANCELLED.value


def test_cancel_all_videos_route_delegates_to_the_video_task_service(monkeypatch) -> None:
    """The toolbar bulk control must invoke the project-wide cancellation service."""

    expected = {"project_id": "project-1", "cancelled_count": 2, "cancelled_tasks": []}
    monkeypatch.setattr(
        video_history_routes.task_service,
        "cancel_all_shot_videos",
        lambda project_id: {**expected, "project_id": project_id},
    )

    assert video_history_routes.cancel_project_videos("project-1") == expected


def test_cancel_all_videos_route_is_exposed_over_http(monkeypatch) -> None:
    """The frontend can POST the project-level cancellation endpoint."""

    monkeypatch.setattr(
        video_history_routes.task_service,
        "cancel_all_shot_videos",
        lambda project_id: {
            "project_id": project_id,
            "cancelled_count": 3,
            "cancelled_tasks": [],
            "provider_cancel_errors": [],
        },
    )

    response = TestClient(app).post("/api/projects/project-3/videos/cancel")

    assert response.status_code == 202
    assert response.json()["cancelled_count"] == 3
