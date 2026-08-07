"""Regression coverage for dialog-triggered screenplay continuation."""

import pytest

from src.api import expanded_script_routes
from src.application.task_service import TaskService
from src.domain.models import GenerationStatus, ProjectCreate
from src.infrastructure.sqlite_repository import SQLiteRepository
from src.llm_service.planner import ScriptPlanner


class ContinuationPlanner:
    """Deterministic planner used to verify append-only continuation tasks."""

    def __init__(self) -> None:
        self.continuation_inputs: list[tuple[str, str]] = []

    def expand_script(self, script: str) -> str:
        """Create a stored screenplay for the initial decomposition test step."""

        return f"{script}\n\n" + "第1场：人物推进冲突并留下悬念。\n" * 4_000

    def plan(self, script: str) -> dict:
        """Return one stable shot so the test can detect accidental rebuilding."""

        return {
            "episodes": [{"name": "第1集", "shots": [{"title": "开场", "original_text": script[:80]}]}],
            "assets": [],
        }

    def continue_expanded_script(self, script: str, expanded_script: str, **kwargs) -> str:
        """Append a visible installment and exercise stream/checkpoint callbacks."""

        self.continuation_inputs.append((script, expanded_script))
        continued = f"{expanded_script}\n\n" + "续写场景：危机升级，主角做出新的选择。\n" * 20
        kwargs["stream"](continued)
        kwargs["checkpoint"](continued, len(continued), len(continued))
        return continued


class ContinuationAgent:
    """Minimal streamed provider double for the real planner continuation method."""

    def execute_skill(self, _name: str, _arguments: dict) -> dict:
        """Provide the script-writer instruction without any remote calls."""

        return {"instruction": "保持人物动机与前文一致"}

    async def completion_stream(self, _messages, **_kwargs):
        """Yield enough screenplay prose to pass continuation validation."""

        yield "续写正文，冲突持续升级。" * 40


def test_continuation_appends_script_without_rebuilding_shots(tmp_path) -> None:
    """A dialog continuation updates only screenplay text and its own durable task."""

    repository = SQLiteRepository(tmp_path / "continued-screenplay.db")
    planner = ContinuationPlanner()
    service = TaskService(repository, planner)
    project = service.create_project(
        ProjectCreate(name="继续扩写", script="林岩在旧宅地下室发现一把铜钥匙。")
    )
    service.decompose_project(project["task_id"], project["id"])
    before = service.get_expanded_script(project["id"])["expanded_script"]
    shot_id = service.get_project(project["id"])["shots"][0]["id"]

    task = service.continue_expanded_script(project["id"])
    repeated = service.continue_expanded_script(project["id"])

    assert task["type"] == "script_expansion"
    assert task["status"] == GenerationStatus.GENERATING.value
    assert repeated["id"] == task["id"]
    assert service.get_expanded_script(project["id"])["expanded_script_generating"] is True

    service.run_expanded_script_continuation(task["id"], project["id"])

    screenplay = service.get_expanded_script(project["id"])
    saved_task = repository.get_task(task["id"])
    assert screenplay["expanded_script"].startswith(before)
    assert "续写场景" in screenplay["expanded_script"]
    assert screenplay["expanded_script_task_status"] == GenerationStatus.SUCCEEDED.value
    assert saved_task is not None and saved_task["status"] == GenerationStatus.SUCCEEDED.value
    assert planner.continuation_inputs == [("林岩在旧宅地下室发现一把铜钥匙。", before)]
    assert service.get_project(project["id"])["shots"][0]["id"] == shot_id
    assert service.get_project(project["id"])["status"] == GenerationStatus.SUCCEEDED.value


def test_continuation_blocks_manual_script_edits_while_running(tmp_path) -> None:
    """A creator cannot overwrite the draft between enqueue and worker checkpoint."""

    repository = SQLiteRepository(tmp_path / "continued-screenplay-edit.db")
    service = TaskService(repository, ContinuationPlanner())
    project = service.create_project(
        ProjectCreate(name="继续扩写保护", script="林岩带着铜钥匙进入旧宅地下室。")
    )
    service.decompose_project(project["task_id"], project["id"])
    service.continue_expanded_script(project["id"])

    with pytest.raises(ValueError, match="后台生成"):
        service.update_project_scripts(project["id"], "新的原始剧本内容。", "新的扩写剧本内容。")


def test_cancelling_continuation_keeps_the_existing_storyboard_succeeded(tmp_path) -> None:
    """Stopping an optional continuation must not cancel the completed project."""

    repository = SQLiteRepository(tmp_path / "continued-screenplay-cancel.db")
    service = TaskService(repository, ContinuationPlanner())
    project = service.create_project(
        ProjectCreate(name="取消继续扩写", script="林岩带着铜钥匙进入旧宅地下室。")
    )
    service.decompose_project(project["task_id"], project["id"])
    task = service.continue_expanded_script(project["id"])

    cancelled = service.cancel_expanded_script(project["id"])

    assert cancelled["id"] == task["id"]
    assert cancelled["status"] == GenerationStatus.CANCELLED.value
    assert service.get_project(project["id"])["status"] == GenerationStatus.SUCCEEDED.value


def test_real_planner_continuation_forces_one_new_provider_request(monkeypatch) -> None:
    """A finished draft must still call the LLM again when the creator continues it."""

    planner = ScriptPlanner()
    planner.EXPANDED_SCRIPT_TARGET_CHARS = 100
    planner.EXPANDED_SCRIPT_MAX_CHARS = 1_000
    planner.EXPANDED_SCRIPT_CHUNK_CHARS = 300
    agent = ContinuationAgent()
    checkpoints: list[str] = []
    streams: list[str] = []
    monkeypatch.setattr(planner, "_agent", lambda *_args: agent)

    existing = "已生成剧本。" * 50
    continued = planner.continue_expanded_script(
        "林岩进入旧宅寻找铜钥匙。",
        existing,
        existing_outline="人物关系与主线冲突。",
        checkpoint=lambda value, _written, _target: checkpoints.append(value),
        stream=streams.append,
    )

    assert continued.startswith(existing)
    assert len(continued) > len(existing)
    assert checkpoints == [continued]
    assert streams[-1] == continued


def test_continue_endpoint_delegates_to_the_expansion_service(monkeypatch) -> None:
    """The new route must enqueue only the continuation task service action."""

    expected = {"id": "task-1", "type": "script_expansion", "status": "生成中"}
    monkeypatch.setattr(
        expanded_script_routes.task_service,
        "continue_expanded_script",
        lambda project_id: {**expected, "project_id": project_id},
    )

    assert expanded_script_routes.continue_project_expanded_script("project-1") == {
        **expected,
        "project_id": "project-1",
    }
