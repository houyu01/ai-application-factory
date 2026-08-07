"""Regression coverage for concurrent durable task execution."""

from threading import Event, Lock

import pytest

from src.application.task_service import TaskService
from src.application.task_worker import DurableTaskWorker
from src.domain.models import GenerationStatus, ProjectCreate
from src.infrastructure.sqlite_repository import SQLiteRepository


class BlockingPlanner:
    """Hold two decompositions open so the worker's parallel slots are observable."""

    def __init__(self) -> None:
        self.second_started = Event()
        self.release = Event()
        self._lock = Lock()
        self.calls: list[str] = []

    def plan(self, script: str) -> dict:
        """Wait until both tasks are running, then return the smallest valid plan."""

        with self._lock:
            self.calls.append(script)
            if len(self.calls) == 2:
                self.second_started.set()
        self.release.wait(timeout=5)
        return {"episodes": [{"name": "第1集", "shots": []}], "assets": []}


class EmptyGameRepository:
    """Provide the game-worker boundary while this test only exercises drama tasks."""

    @staticmethod
    def claim_next_runnable_task():
        """Report that there is no game work to claim."""

        return None


class EmptyGameService:
    """Prevent game-task polling from affecting the drama-worker concurrency test."""

    repository = EmptyGameRepository()


def test_durable_worker_processes_two_drama_tasks_in_parallel(tmp_path) -> None:
    """Two task rows must enter planning before either blocked plan completes."""

    planner = BlockingPlanner()
    service = TaskService(SQLiteRepository(tmp_path / "parallel-worker.db"), planner)
    first = service.create_project(ProjectCreate(name="第一部", script="第一部短剧的剧本内容足够长。"))
    second = service.create_project(ProjectCreate(name="第二部", script="第二部短剧的剧本内容足够长。"))
    worker = DurableTaskWorker(service, EmptyGameService())

    worker.start()
    try:
        assert worker.concurrency == 2
        assert planner.second_started.wait(timeout=5)
        assert set(planner.calls) == {"第一部短剧的剧本内容足够长。", "第二部短剧的剧本内容足够长。"}
    finally:
        planner.release.set()
        worker.stop()

    assert service.repository.get_task(first["task_id"])["status"] == "生成成功"
    assert service.repository.get_task(second["task_id"])["status"] == "生成成功"


def test_model_queue_settings_persist_and_apply_independent_concurrency(tmp_path) -> None:
    """Each model card must configure only its own durable task queue."""

    service = TaskService(SQLiteRepository(tmp_path / "worker-setting.db"), object())
    service._probe_model_config = lambda config: None
    configs = {
        "language": {
            "kind": "language", "model": "language-model", "models": ["language-model"],
            "endpoint": "https://example.test", "api_key": "test-key",
        },
        "multimodal": {
            "kind": "multimodal", "model": "image-model", "models": ["image-model"],
            "endpoint": "https://example.test", "api_key": "test-key",
        },
        "video": {
        "kind": "video", "model": "video-model", "models": ["video-model"],
        "endpoint": "https://example.test", "create_url": "https://example.test/create",
        "query_url": "https://example.test/query/{id}", "api_key": "test-key",
        },
        "audio": {
            "kind": "audio", "model": "audio-model", "models": ["audio-model"],
            "endpoint": "https://example.test", "api_key": "test-key",
        },
    }
    configured = {"language": 3, "multimodal": 4, "video": 5, "audio": 6}
    saved = {
        kind: service.save_model_config({**config, "generation_concurrency": configured[kind]})
        for kind, config in configs.items()
    }
    reloaded = TaskService(SQLiteRepository(tmp_path / "worker-setting.db"), object())
    worker = DurableTaskWorker(reloaded, EmptyGameService())

    assert {kind: item["generation_concurrency"] for kind, item in saved.items()} == configured
    assert {
        kind: item["generation_concurrency"]
        for kind, item in reloaded.get_model_configs().items()
    } == configured
    assert worker.queue_concurrency == {
        "language": 3, "image": 4, "video": 5, "audio": 6,
    }
    assert worker.concurrency == 5
    assert worker.set_queue_concurrency("language", 2) == 2
    assert worker.queue_concurrency["language"] == 2
    assert worker.set_concurrency(2) == 2
    with pytest.raises(ValueError, match="1 到 8"):
        service.save_model_config({**configs["audio"], "generation_concurrency": 9})


def test_video_queue_does_not_submit_more_than_its_remote_concurrency(tmp_path) -> None:
    """Remote video work must occupy a video slot until its provider task ends."""

    repository = SQLiteRepository(tmp_path / "video-queue.db")
    service = TaskService(repository, object())
    project = service.create_project(
        ProjectCreate(name="视频队列", script="林岩走进旧宅，发现墙上留有新的记号。")
    )
    first = repository.create_task(project["id"], "shot_video", "shot-1")
    second = repository.create_task(project["id"], "shot_video", "shot-2")
    repository.update_task_status(first["id"], GenerationStatus.GENERATING)
    repository.update_task_status(second["id"], GenerationStatus.GENERATING)
    repository.update_task_progress(first["id"], provider_task_id="provider-task-1")

    claimed = repository.claim_next_runnable_task(
        task_types={"shot_video"}, max_active_tasks=1
    )
    assert claimed is not None and claimed["id"] == first["id"]
    assert repository.claim_next_runnable_task(
        task_types={"shot_video"}, max_active_tasks=1
    ) is None

    repository.update_task_status(first["id"], GenerationStatus.SUCCEEDED)
    next_task = repository.claim_next_runnable_task(
        task_types={"shot_video"}, max_active_tasks=1
    )
    assert next_task is not None and next_task["id"] == second["id"]
