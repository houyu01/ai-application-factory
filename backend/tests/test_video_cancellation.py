"""Regression coverage for cancelling durable short-drama video tasks."""

from src.application.task_service import TaskService
from src.domain.models import GenerationStatus, ProjectCreate
from src.infrastructure.sqlite_repository import SQLiteRepository


class OneShotPlanner:
    """Supply one shot so cancellation tests only exercise the video flow."""

    def plan(self, script: str) -> dict:
        return {
            "episodes": [{"name": "第1集", "shots": [{"title": "开场", "original_text": script}]}],
            "assets": [],
        }


class TwoShotPlanner:
    """Supply two shots to validate project-wide video cancellation."""

    def plan(self, script: str) -> dict:
        return {
            "episodes": [{"name": "第1集", "shots": [
                {"title": "开场", "original_text": script},
                {"title": "转场", "original_text": script},
            ]}],
            "assets": [],
        }


def test_cancel_shot_video_stops_local_task_version_and_ark_request(tmp_path, monkeypatch) -> None:
    """A running shot video must be durable-cancelled before its Ark deletion."""

    service = TaskService(SQLiteRepository(tmp_path / "video-cancel.db"), OneShotPlanner())
    project = service.create_project(
        ProjectCreate(name="取消视频", script="林岩在旧宅门前发现一把铜钥匙。")
    )
    service.decompose_project(project["task_id"], project["id"])
    shot = service.get_project(project["id"])["shots"][0]
    task = service.enqueue("shot_video", project["id"], shot["id"])
    service.repository.update_task_progress(
        task["id"], provider_task_id="ark-video-task", stage="provider_submitted"
    )
    provider_task_ids: list[str] = []
    monkeypatch.setattr(
        service,
        "_cancel_remote_video_task",
        lambda _project, task_id: provider_task_ids.append(task_id),
    )

    cancelled = service.cancel_shot_video(project["id"], shot["id"])

    saved_task = service.repository.get_task(task["id"])
    saved_shot = service.repository.get_shot(project["id"], shot["id"])
    version_id = task["input_snapshot"]["version_id"]
    version = service.repository.list_shot_versions(project["id"], shot["id"])[0]
    assert cancelled["status"] == GenerationStatus.CANCELLED.value
    assert cancelled["provider_cancelled"] is True
    assert provider_task_ids == ["ark-video-task"]
    assert saved_task is not None and saved_task["status"] == GenerationStatus.CANCELLED.value
    assert saved_shot is not None and saved_shot["status"] == GenerationStatus.CANCELLED.value
    assert version["id"] == version_id
    assert version["status"] == GenerationStatus.CANCELLED.value
    assert version["completed_at"]


def test_cancel_queued_shot_video_skips_the_provider_call(tmp_path, monkeypatch) -> None:
    """Tasks cancelled before Ark submission must never create a remote request."""

    service = TaskService(SQLiteRepository(tmp_path / "queued-video-cancel.db"), OneShotPlanner())
    project = service.create_project(
        ProjectCreate(name="取消队列视频", script="林岩带着铜钥匙走进地下室。")
    )
    service.decompose_project(project["task_id"], project["id"])
    shot = service.get_project(project["id"])["shots"][0]
    service.enqueue("shot_video", project["id"], shot["id"])
    monkeypatch.setattr(
        service,
        "_cancel_remote_video_task",
        lambda *_args: (_ for _ in ()).throw(AssertionError("不应调用方舟取消接口")),
    )

    cancelled = service.cancel_shot_video(project["id"], shot["id"])

    assert cancelled["status"] == GenerationStatus.CANCELLED.value
    assert cancelled["provider_cancelled"] is False


def test_cancel_all_shot_videos_cancels_each_running_task(tmp_path, monkeypatch) -> None:
    """Bulk cancellation must preserve task, shot, version, and Ark cleanup behavior."""

    service = TaskService(SQLiteRepository(tmp_path / "bulk-video-cancel.db"), TwoShotPlanner())
    project = service.create_project(
        ProjectCreate(name="批量取消视频", script="林岩带着铜钥匙穿过空荡的长廊。")
    )
    service.decompose_project(project["task_id"], project["id"])
    shots = service.get_project(project["id"])["shots"]
    first_task = service.enqueue("shot_video", project["id"], shots[0]["id"])
    second_task = service.enqueue("shot_video", project["id"], shots[1]["id"])
    service.repository.update_task_progress(
        first_task["id"], provider_task_id="ark-bulk-task", stage="provider_submitted"
    )
    provider_task_ids: list[str] = []
    monkeypatch.setattr(
        service,
        "_cancel_remote_video_task",
        lambda _project, task_id: provider_task_ids.append(task_id),
    )

    result = service.cancel_all_shot_videos(project["id"])

    assert result["cancelled_count"] == 2
    assert result["provider_cancel_errors"] == []
    assert provider_task_ids == ["ark-bulk-task"]
    assert service.repository.get_task(first_task["id"])["status"] == GenerationStatus.CANCELLED.value
    assert service.repository.get_task(second_task["id"])["status"] == GenerationStatus.CANCELLED.value
    assert all(shot["status"] == GenerationStatus.CANCELLED.value for shot in service.get_project(project["id"])["shots"])
