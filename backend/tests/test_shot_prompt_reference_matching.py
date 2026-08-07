"""Regression coverage for prompt regeneration reference-image matching."""

from src.application.task_service import TaskService
from src.domain.models import ProjectCreate
from src.infrastructure.sqlite_repository import SQLiteRepository
from src.llm_service.planner import ScriptPlanner


class _Planner:
    def plan(self, script: str) -> dict:
        return {
            "episodes": [{"name": "第1集", "shots": [{"title": "初始分镜", "original_text": script}]}],
            "assets": [],
        }


def _ready_asset(service: TaskService, project_id: str, asset_type: str, name: str, *, metadata: dict | None = None) -> dict:
    """Create a generated material image available to a regenerated prompt."""

    asset = service.repository.create_asset(
        project_id, asset_type, name, prompt=f"{name}的视觉描述", metadata=metadata
    )
    return service.repository.set_asset_image(
        project_id, asset["id"], f"https://cdn.example/{asset['id']}.png"
    )


def test_regenerated_prompt_replaces_stale_references_with_matching_ready_images(tmp_path) -> None:
    """Regeneration uses the latest script and only ready material images, including placeholders."""

    service = TaskService(SQLiteRepository(tmp_path / "matching.db"), _Planner())
    created = service.create_project(
        ProjectCreate(name="参考图匹配", script="林岩走进雨夜的旧城，寻找失落的铜钥匙。")
    )
    service.decompose_project(created["task_id"], created["id"])
    project = service.get_project(created["id"])
    shot = project["shots"][0]
    service.repository.update_shot(
        project["id"], shot["id"], title="雨夜旧巷", original_text="林岩在雨夜的旧巷拾起铜钥匙。"
    )
    character = _ready_asset(service, project["id"], "character", "林岩")
    scene = _ready_asset(service, project["id"], "scene", "雨夜旧巷")
    prop = _ready_asset(service, project["id"], "prop", "铜钥匙")
    placeholder = _ready_asset(
        service, project["id"], "placeholder", "雨夜旧巷构图",
        metadata={"render_mode": "generated_composite", "scene_name": "雨夜旧巷"},
    )
    _ready_asset(service, project["id"], "character", "苏晚")
    unready = service.repository.create_asset(project["id"], "prop", "密信", prompt="泛黄密信")

    task = service.enqueue("shot_prompt", project["id"], shot["id"])
    service.run_shot_prompt(task["id"], project["id"], shot["id"])
    saved = service.get_project(project["id"])["shots"][0]

    assert set(saved["reference_asset_ids"]) == {
        character["id"], scene["id"], prop["id"], placeholder["id"]
    }
    assert unready["id"] not in saved["reference_asset_ids"]
    assert "自动匹配参考图：" in saved["prompt"]
    assert "@图" in saved["prompt"]

    current_shot = service.repository.get_shot(project["id"], shot["id"])
    matched = ScriptPlanner.select_ready_shot_reference_assets(
        current_shot or {}, [character, scene, prop, placeholder]
    )
    nodes = ScriptPlanner().generate_shot_prompt_rich({}, current_shot or {}, matched)
    nodes = ScriptPlanner.ensure_shot_references(nodes, matched)
    assert {node["asset_id"] for node in nodes if node["type"] == "reference"} == {
        character["id"], scene["id"], prop["id"], placeholder["id"]
    }
