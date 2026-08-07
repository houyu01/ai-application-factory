"""Regression coverage for persisted long-form short-drama screenplays."""

from src.application.task_service import TaskService
from src.application.task_worker import DurableTaskWorker
from src.api import expanded_script_routes
from src.api.router import _task_status_payload
from src.domain.models import GenerationStatus, ProjectCreate
from src.infrastructure.sqlite_repository import SQLiteRepository
from src.llm_service.planner import ScriptPlanner
from src.llm_service.planner_expansion_mixin import RetryableExpansionError


class ExpandingPlanner:
    """Deterministically model the expansion-before-decomposition production flow."""

    def __init__(self) -> None:
        self.planned_script = ""

    def expand_script(self, script: str) -> str:
        """Return a repeatable screenplay large enough to exercise persistence."""

        return f"{script}\n\n" + "第1场：人物推动剧情并留下新的悬念。\n" * 6_000

    def plan(self, script: str) -> dict:
        """Record the exact screenplay the subsequent decomposition receives."""

        self.planned_script = script
        return {
            "episodes": [{"name": "第1集", "shots": [{"title": "开场", "original_text": script[:80]}]}],
            "assets": [
                {"type": "character", "name": "林岩", "prompt": "推动真相的主角"},
                {"type": "scene", "name": "旧宅", "prompt": "故事开始的旧宅"},
                {"type": "prop", "name": "铜钥匙", "prompt": "开启谜团的钥匙"},
            ],
        }


class ResumablePlanner(ScriptPlanner):
    """Simulate a process interruption after a persisted expansion checkpoint."""

    EXPANDED_SCRIPT_TARGET_CHARS = 50

    def __init__(self) -> None:
        super().__init__()
        self.interrupt_once = True
        self.resume_inputs: list[str] = []

    def expand_script(self, script: str, **kwargs) -> str:
        """Checkpoint a partial result once, then finish from it after recovery."""

        existing = str(kwargs.get("existing_script") or "")
        checkpoint = kwargs.get("checkpoint")
        self.resume_inputs.append(existing)
        partial = existing or "已保存片段" * 5
        if checkpoint:
            checkpoint(partial, len(partial), self.EXPANDED_SCRIPT_TARGET_CHARS)
        if self.interrupt_once:
            self.interrupt_once = False
            raise KeyboardInterrupt
        return partial + "续写内容" * 10

    def plan(self, script: str, options=None) -> dict:
        """Return one minimal shot after the recovered screenplay is complete."""

        return {
            "episodes": [{"name": "第1集", "shots": [{"title": "开场", "original_text": script[:80]}]}],
            "assets": [],
        }


class RetryingExpansionAgent:
    """Provide a deterministic temporary connection failure for planner retry coverage."""

    def __init__(self) -> None:
        self.calls = 0

    def execute_skill(self, _name: str, _arguments: dict) -> dict:
        """Return compact skill instructions without calling a provider."""

        return {"instruction": "按剧情推进"}

    def completion(self, _messages, **_kwargs) -> str:
        """Return the outline before the streamed installments begin."""

        self.calls += 1
        if self.calls == 1:
            return "故事大纲"
        raise AssertionError("正文应通过 completion_stream 输出")

    async def completion_stream(self, _messages, **_kwargs):
        """Fail once, then produce each installment in two visible deltas."""

        self.calls += 1
        if self.calls == 2:
            raise ConnectionError("temporary connection loss")
        yield "剧" * 100
        yield "剧" * 100


class TemporarilyOfflinePlanner(ScriptPlanner):
    """Raise a retryable provider failure after the request-level retries are exhausted."""

    def expand_script(self, _script: str, **_kwargs) -> str:
        """Represent a provider that remains unavailable for this worker attempt."""

        raise RetryableExpansionError("扩写剧本第 1 节请求语言模型失败（已尝试 3 次）：Connection error.")


class PreviewRetentionPlanner(ScriptPlanner):
    """Emit a preview then stop after a checkpoint to test browser recovery state."""

    EXPANDED_SCRIPT_TARGET_CHARS = 400

    def expand_script(self, _script: str, **kwargs) -> str:
        """Persist a checkpoint only after the live preview callback has fired."""

        partial = "正在流式输出的正文。" * 50
        kwargs["stream"](partial)
        kwargs["checkpoint"](partial, len(partial), self.EXPANDED_SCRIPT_TARGET_CHARS)
        raise KeyboardInterrupt


class ProjectDeletingPlanner(ScriptPlanner):
    """Simulate a creator deleting a project during a streamed expansion."""

    def __init__(self, repository: SQLiteRepository) -> None:
        super().__init__()
        self.repository = repository
        self.project_id = ""

    def expand_script(self, _script: str, **kwargs) -> str:
        """Remove the task owner before the next stream-preview persistence."""

        self.repository.delete_drama(self.project_id)
        raise RuntimeError("故事大纲请求语言模型失败（已尝试 3 次）：'Task not found: deleted-task'")


class PreviewFailingPlanner(ScriptPlanner):
    """Fail before the next stream event to verify that prior text is not cleared."""

    def expand_script(self, _script: str, **_kwargs) -> str:
        """Simulate an expansion failure after a prior worker checkpoint exists."""

        raise RuntimeError("语言模型暂不可用")


def test_expanded_screenplay_is_persisted_and_used_for_decomposition(tmp_path) -> None:
    """The original script stays intact while planning receives the stored expansion."""

    planner = ExpandingPlanner()
    repository = SQLiteRepository(tmp_path / "expanded-screenplay.db")
    service = TaskService(repository, planner)
    original = "林岩回到荒废旧宅，在门缝里发现一把铜钥匙。"
    project = service.create_project(ProjectCreate(name="旧宅谜影", script=original))
    pending = service.get_expanded_script(project["id"])
    assert pending["expanded_script_generating"] is True
    assert pending["expanded_script_cancellable"] is True

    service.decompose_project(project["task_id"], project["id"])

    stored = service.get_expanded_script(project["id"])
    saved_project = service.get_project(project["id"])
    assert stored["expanded_script"].startswith(original)
    assert stored["expanded_script_length"] >= 50_000
    assert stored["expanded_script_generating"] is False
    assert stored["expanded_script_cancellable"] is False
    assert stored["original_script_length"] == len(original)
    assert planner.planned_script == stored["expanded_script"]
    assert saved_project["script"] == original
    assert "expanded_script" not in saved_project
    assert "expanded_script" not in service.list_projects()[0]
    assert saved_project["status"] == GenerationStatus.SUCCEEDED.value
    assert saved_project["tasks"][0]["stage"] == "正在保存分镜和素材"
    assert saved_project["tasks"][0]["result"]["expanded_script_length"] == len(planner.planned_script)

    reopened = SQLiteRepository(tmp_path / "expanded-screenplay.db")
    assert reopened.get_expanded_script(project["id"])["expanded_script"] == planner.planned_script


def test_expanded_screenplay_endpoint_uses_the_dedicated_service_method(monkeypatch) -> None:
    """The dialog route must not request the full project aggregate."""

    expected = {"project_id": "project-1", "expanded_script": "扩写正文", "expanded_script_length": 4}
    monkeypatch.setattr(
        expanded_script_routes.task_service,
        "get_expanded_script",
        lambda project_id: {**expected, "project_id": project_id},
    )

    assert expanded_script_routes.get_project_expanded_script("project-1") == expected


def test_cancel_expanded_screenplay_endpoint_uses_the_dedicated_service_method(monkeypatch) -> None:
    """The dialog cancellation route delegates to the screenplay task service."""

    expected = {"id": "task-1", "status": GenerationStatus.CANCELLED.value}
    monkeypatch.setattr(
        expanded_script_routes.task_service,
        "cancel_script_decomposition",
        lambda project_id: {**expected, "project_id": project_id},
    )

    assert expanded_script_routes.cancel_project_expanded_script("project-1") == {
        **expected,
        "project_id": "project-1",
    }


def test_expansion_retries_a_temporary_connection_error(monkeypatch) -> None:
    """One transient model disconnect must not discard an otherwise valid expansion."""

    planner = ScriptPlanner()
    planner.EXPANDED_SCRIPT_TARGET_CHARS = 400
    planner.EXPANDED_SCRIPT_CHUNK_CHARS = 200
    planner.EXPANDED_SCRIPT_MAX_CHUNKS = 3
    planner.EXPANDED_SCRIPT_MAX_RETRIES = 2
    agent = RetryingExpansionAgent()
    delays: list[int] = []
    checkpoints: list[tuple[int, int, int]] = []
    previews: list[str] = []
    monkeypatch.setattr(planner, "_agent", lambda *_args: agent)
    monkeypatch.setattr("src.llm_service.planner_expansion_mixin.sleep", delays.append)

    screenplay = planner.expand_script(
        "林岩进入旧宅。",
        checkpoint=lambda value, written, target: checkpoints.append((len(value), written, target)),
        stream=previews.append,
    )

    assert screenplay is not None and len(screenplay) >= 400
    assert agent.calls == 4
    assert delays == [1]
    assert checkpoints == [(200, 200, 400), (402, 400, 400)]
    assert [len(value) for value in previews] == [100, 200, 100, 200, 302, 402]


def test_expansion_resumes_from_a_persisted_checkpoint_after_interruption(tmp_path) -> None:
    """A restarted worker must continue with the stored partial screenplay."""

    repository = SQLiteRepository(tmp_path / "resumable-screenplay.db")
    planner = ResumablePlanner()
    service = TaskService(repository, planner)
    project = service.create_project(ProjectCreate(name="断点续写", script="林岩独自进入荒废旧宅寻找铜钥匙。"))

    try:
        service.decompose_project(project["task_id"], project["id"])
    except KeyboardInterrupt:
        pass

    pending = service.get_expanded_script(project["id"])
    assert pending["expanded_script"] == "已保存片段" * 5
    assert pending["expanded_script_generating"] is True

    service.decompose_project(project["task_id"], project["id"])

    completed = service.get_expanded_script(project["id"])
    assert planner.resume_inputs == ["", "已保存片段" * 5]
    assert completed["expanded_script"].startswith("已保存片段" * 5)
    assert completed["expanded_script_generating"] is False


def test_worker_reschedules_a_retryable_expansion_failure(tmp_path) -> None:
    """The durable worker keeps a temporary expansion failure generating for recovery."""

    repository = SQLiteRepository(tmp_path / "retryable-screenplay.db")
    service = TaskService(repository, TemporarilyOfflinePlanner())
    project = service.create_project(ProjectCreate(name="连接恢复", script="林岩独自进入荒废旧宅寻找铜钥匙。"))
    task = repository.claim_next_runnable_task()
    assert task is not None

    DurableTaskWorker(service, object())._run_drama_task(task)

    saved_task = repository.get_task(project["task_id"])
    assert saved_task is not None
    assert saved_task["status"] == GenerationStatus.GENERATING.value
    assert saved_task["next_poll_at"]
    assert "连接暂时不可用" in saved_task["stage"]
    assert service.get_project(project["id"])["status"] == GenerationStatus.GENERATING.value


def test_expanded_script_endpoint_returns_the_active_stream_preview(tmp_path) -> None:
    """The dialog receives live preview text while the durable task is still generating."""

    repository = SQLiteRepository(tmp_path / "stream-preview.db")
    service = TaskService(repository, ExpandingPlanner())
    project = service.create_project(ProjectCreate(name="流式展示", script="林岩独自进入荒废旧宅寻找铜钥匙。"))
    task = repository.get_task(project["task_id"])
    assert task is not None
    repository.update_task_input_snapshot(
        project["task_id"], {**(task["input_snapshot"] or {}), "expanded_script_preview": "正在流式输出的正文"}
    )

    payload = service.get_expanded_script(project["id"])

    assert payload["expanded_script_generating"] is True
    assert payload["expanded_script_preview"] == "正在流式输出的正文"
    assert payload["expanded_script_length"] == len("正在流式输出的正文")


def test_expanded_script_endpoint_retains_preview_after_expansion_fails(tmp_path) -> None:
    """A failed task must keep its final streamed text recoverable in the dialog."""

    repository = SQLiteRepository(tmp_path / "failed-stream-preview.db")
    service = TaskService(repository, ExpandingPlanner())
    project = service.create_project(ProjectCreate(name="失败预览", script="林岩独自进入旧宅寻找铜钥匙。"))
    task = repository.get_task(project["task_id"])
    assert task is not None
    preview = "故事圣经已实时生成的内容。" * 20
    repository.update_task_input_snapshot(
        project["task_id"], {**(task["input_snapshot"] or {}), "expanded_script_preview": preview}
    )
    repository.update_task_status(
        project["task_id"], GenerationStatus.FAILED, error_message="语言模型暂不可用"
    )

    payload = service.get_expanded_script(project["id"])

    assert payload["expanded_script_generating"] is False
    assert payload["expanded_script_task_status"] == GenerationStatus.FAILED.value
    assert payload["expanded_script_preview"] == preview
    assert payload["expanded_script_length"] == len(preview)


def test_retry_does_not_clear_the_last_stream_preview(tmp_path) -> None:
    """The first retry request must leave recoverable text visible until new output arrives."""

    repository = SQLiteRepository(tmp_path / "retry-preview-retention.db")
    service = TaskService(repository, PreviewFailingPlanner())
    project = service.create_project(ProjectCreate(name="续写保留", script="林岩独自进入旧宅寻找铜钥匙。"))
    task = repository.get_task(project["task_id"])
    assert task is not None
    preview = "已经生成的剧本文字。" * 20
    repository.update_task_input_snapshot(
        project["task_id"], {**(task["input_snapshot"] or {}), "expanded_script_preview": preview}
    )

    service.decompose_project(project["task_id"], project["id"])

    saved_task = repository.get_task(project["task_id"])
    assert saved_task is not None
    assert saved_task["status"] == GenerationStatus.FAILED.value
    assert saved_task["input_snapshot"]["expanded_script_preview"] == preview


def test_failed_script_can_be_requeued_without_losing_its_checkpoint(tmp_path) -> None:
    """The banner retry action must retain screenplay and story-bible recovery data."""

    repository = SQLiteRepository(tmp_path / "retry-failed-script.db")
    service = TaskService(repository, ExpandingPlanner())
    project = service.create_project(ProjectCreate(name="失败后续写", script="林岩独自进入旧宅寻找铜钥匙。"))
    task = repository.get_task(project["task_id"])
    assert task is not None
    snapshot = {
        **(task["input_snapshot"] or {}),
        "expanded_script_preview": "已保存的正文。",
        "story_bible": "已保存的故事圣经。",
    }
    repository.update_task_input_snapshot(project["task_id"], snapshot)
    repository.update_task_status(project["task_id"], GenerationStatus.FAILED, error_message="请求超时")

    retried = service.retry_script_decomposition(project["id"])

    assert retried["id"] == project["task_id"]
    assert retried["status"] == GenerationStatus.GENERATING.value
    assert retried["error_message"] is None
    assert retried["input_snapshot"] == snapshot
    assert service.get_project(project["id"])["status"] == GenerationStatus.GENERATING.value


def test_task_polling_payload_includes_only_a_bounded_stream_preview() -> None:
    """The task banner must receive stream text without exposing task input."""

    full_preview = "故事圣经正文" * 1_000
    payload = _task_status_payload(
        {
            "id": "task-1",
            "type": "script_decomposition",
            "input_snapshot": {"script": "不可暴露", "expanded_script_preview": full_preview},
        }
    )

    preview = payload["input_snapshot"]["expanded_script_preview"]
    assert len(preview) <= 3_232
    assert "已省略" in preview
    assert "script" not in payload["input_snapshot"]


def test_checkpoint_retains_the_stream_preview_for_task_polling(tmp_path) -> None:
    """Saving a recovery checkpoint must not blank the active task banner."""

    repository = SQLiteRepository(tmp_path / "preview-checkpoint.db")
    service = TaskService(repository, PreviewRetentionPlanner())
    project = service.create_project(ProjectCreate(name="实时故事圣经", script="林岩进入旧宅寻找铜钥匙。"))

    try:
        service.decompose_project(project["task_id"], project["id"])
    except KeyboardInterrupt:
        pass

    task = repository.get_task(project["task_id"])
    assert task is not None
    assert task["input_snapshot"]["expanded_script_preview"].startswith("正在流式输出的正文。")


def test_deleting_project_during_streaming_expansion_stops_without_failure_update(tmp_path) -> None:
    """A deleted project must not trigger a second task/project lookup failure."""

    repository = SQLiteRepository(tmp_path / "deleted-during-stream.db")
    planner = ProjectDeletingPlanner(repository)
    service = TaskService(repository, planner)
    project = service.create_project(ProjectCreate(name="删除中的剧本", script="林岩独自进入旧宅寻找铜钥匙。"))
    planner.project_id = project["id"]

    service.decompose_project(project["task_id"], project["id"])

    assert repository.get_drama(project["id"]) is None
    assert repository.get_task(project["task_id"]) is None


def test_script_dialog_edits_persist_without_rebuilding_shots(tmp_path) -> None:
    """Editing either text area retains the existing decomposition until rerun."""

    repository = SQLiteRepository(tmp_path / "script-dialog.db")
    service = TaskService(repository, ExpandingPlanner())
    project = service.create_project(ProjectCreate(name="剧本编辑", script="林岩在旧宅发现铜钥匙。"))
    service.decompose_project(project["task_id"], project["id"])
    shot_id = service.get_project(project["id"])["shots"][0]["id"]

    saved = service.update_project_scripts(
        project["id"], "林岩带着铜钥匙进入旧宅的地下室。", "扩写后剧本正文。"
    )

    assert saved["script"] == "林岩带着铜钥匙进入旧宅的地下室。"
    assert saved["expanded_script"] == "扩写后剧本正文。"
    assert service.get_project(project["id"])["shots"][0]["id"] == shot_id
