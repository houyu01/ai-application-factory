"""Regression coverage for creator-cancelled screenplay expansion."""

import asyncio
import threading

from src.application.task_service import TaskService
from src.application.task_worker import DurableTaskWorker
from src.domain.models import GenerationStatus, ProjectCreate
from src.infrastructure.sqlite_repository import SQLiteRepository
from src.llm_service.planner import ScriptPlanner


class BlockingExpansionAgent:
    """Hold a provider stream open until cancellation closes it."""

    def __init__(self) -> None:
        self.stream_started = threading.Event()
        self.stream_closed = threading.Event()

    def execute_skill(self, _name: str, _arguments: dict) -> dict:
        """Return static instructions so only the provider stream blocks."""

        return {"instruction": "按剧情推进"}

    async def completion_stream(self, _messages, **_kwargs):
        """Wait in a cancellable stream request and record its closure."""

        self.stream_started.set()
        try:
            while True:
                await asyncio.sleep(30)
                yield "不应输出"
        finally:
            self.stream_closed.set()


class BlockingExpansionPlanner(ScriptPlanner):
    """Use the real expansion flow with a stream that only cancellation releases."""

    EXPANDED_SCRIPT_TARGET_CHARS = 200
    EXPANDED_SCRIPT_MAX_CHARS = 400

    def __init__(self, agent: BlockingExpansionAgent) -> None:
        super().__init__()
        self.agent = agent

    def _agent(self, *_args):
        """Supply the deterministic blocking agent to every expansion stage."""

        return self.agent


def test_cancelling_expansion_closes_stream_and_releases_worker(tmp_path) -> None:
    """The dialog cancellation stops a live stream and returns the worker slot."""

    repository = SQLiteRepository(tmp_path / "cancel-screenplay.db")
    agent = BlockingExpansionAgent()
    service = TaskService(repository, BlockingExpansionPlanner(agent))
    project = service.create_project(
        ProjectCreate(name="取消扩写", script="林岩独自进入荒废旧宅寻找铜钥匙。")
    )
    task = repository.claim_next_runnable_task()
    assert task is not None and task["poll_lease_token"]
    worker = DurableTaskWorker(service, object())
    worker_thread = threading.Thread(target=worker._run_drama_task, args=(task,))
    worker_thread.start()
    assert agent.stream_started.wait(timeout=1)

    cancelled = service.cancel_script_decomposition(project["id"])

    worker_thread.join(timeout=2)
    saved_task = repository.get_task(project["task_id"])
    assert not worker_thread.is_alive()
    assert agent.stream_closed.wait(timeout=1)
    assert cancelled["status"] == GenerationStatus.CANCELLED.value
    assert saved_task is not None
    assert saved_task["status"] == GenerationStatus.CANCELLED.value
    assert saved_task["poll_lease_token"] is None
    assert saved_task["poll_lease_until"] is None
    assert service.get_project(project["id"])["status"] == GenerationStatus.CANCELLED.value


def test_cancelling_a_failed_expansion_confirms_it_is_already_stopped(tmp_path) -> None:
    """A stale dialog cancel click must confirm failure without reviving the task."""

    repository = SQLiteRepository(tmp_path / "failed-screenplay.db")
    service = TaskService(repository, ScriptPlanner())
    project = service.create_project(
        ProjectCreate(name="失败后取消", script="林岩独自进入荒废旧宅寻找铜钥匙。")
    )
    repository.update_task_status(
        project["task_id"], GenerationStatus.FAILED, error_message="语言模型请求失败"
    )

    task = service.cancel_script_decomposition(project["id"])
    screenplay = service.get_expanded_script(project["id"])

    assert task["id"] == project["task_id"]
    assert task["status"] == GenerationStatus.FAILED.value
    assert screenplay["expanded_script_generating"] is False
    assert screenplay["expanded_script_cancellable"] is False
