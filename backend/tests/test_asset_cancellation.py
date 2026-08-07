"""Regression coverage for cancelling asset-drawer image generations."""

from fastapi.testclient import TestClient

from src.api import asset_batch_routes
from src.application.task_service import TaskService
from src.domain.models import GenerationStatus, ProjectCreate
from src.infrastructure.sqlite_repository import SQLiteRepository
from src.main import app


class AssetPlanner:
    """Supply character and scene assets for isolated cancellation coverage."""

    def plan(self, script: str) -> dict:
        return {
            "episodes": [{"name": "第1集", "shots": []}],
            "assets": [
                {"type": "character", "name": "林越", "prompt": "青年剑客"},
                {"type": "scene", "name": "青云山", "prompt": "山门演武场"},
            ],
        }


def prepared_service(tmp_path) -> tuple[TaskService, dict, dict, dict]:
    """Create one durable project containing a character and an unrelated scene."""

    service = TaskService(SQLiteRepository(tmp_path / "asset-cancel.db"), AssetPlanner())
    project = service.create_project(
        ProjectCreate(name="取消角色图片", script="林越在青云山的演武场拔剑迎战。")
    )
    service.decompose_project(project["task_id"], project["id"])
    assets = service.get_project(project["id"])["assets"]
    character = next(asset for asset in assets if asset["type"] == "character")
    scene = next(asset for asset in assets if asset["type"] == "scene")
    return service, project, character, scene


def test_cancel_character_images_stops_only_character_base_variant_and_batch_tasks(tmp_path) -> None:
    """The character drawer must leave another tab's active scene image untouched."""

    service, project, character, scene = prepared_service(tmp_path)
    service.repository.create_asset_variant(
        project["id"], character["id"], "战斗形态"
    )
    variant = service.repository.get_asset(project["id"], character["id"])["variants"][0]
    base_task = service.enqueue("asset_image", project["id"], character["id"])
    variant_task = service.enqueue_asset_variant_image(project["id"], character["id"], variant["id"])
    batch_task = service.enqueue_asset_image_batch(project["id"], [character["id"]])
    scene_task = service.enqueue("asset_image", project["id"], scene["id"])

    result = service.cancel_asset_image_tasks(project["id"], "character")

    assert result["cancelled_count"] == 3
    assert service.repository.get_task(base_task["id"])["status"] == GenerationStatus.CANCELLED.value
    assert service.repository.get_task(variant_task["id"])["status"] == GenerationStatus.CANCELLED.value
    assert service.repository.get_task(batch_task["id"])["status"] == GenerationStatus.CANCELLED.value
    saved_character = service.repository.get_asset(project["id"], character["id"])
    assert saved_character["status"] == GenerationStatus.CANCELLED.value
    assert saved_character["variants"][0]["status"] == GenerationStatus.CANCELLED.value
    assert service.repository.get_task(scene_task["id"])["status"] == GenerationStatus.GENERATING.value
    assert service.repository.get_asset(project["id"], scene["id"])["status"] == GenerationStatus.GENERATING.value


def test_cancelled_image_task_does_not_commit_a_late_provider_result(tmp_path, monkeypatch) -> None:
    """A provider response arriving after cancellation cannot revive the asset card."""

    service, project, character, _scene = prepared_service(tmp_path)
    task = service.enqueue("asset_image", project["id"], character["id"])

    def generate_image(_project, _asset) -> str:
        service.cancel_asset_image_tasks(project["id"], "character")
        return "/api/media/late-image.png"

    monkeypatch.setattr(service, "_generate_image_url", generate_image)
    service.run_asset_image(task["id"], project["id"], character["id"])

    assert service.repository.get_task(task["id"])["status"] == GenerationStatus.CANCELLED.value
    assert service.repository.get_asset(project["id"], character["id"])["status"] == GenerationStatus.CANCELLED.value


def test_cancel_character_images_route_delegates_and_is_registered(monkeypatch) -> None:
    """The character drawer's POST action must reach the dedicated service method."""

    expected = {"project_id": "project-1", "asset_type": "character", "cancelled_count": 2}
    monkeypatch.setattr(
        asset_batch_routes.task_service,
        "cancel_asset_image_tasks",
        lambda project_id, asset_type: {**expected, "project_id": project_id, "asset_type": asset_type},
    )

    assert asset_batch_routes.cancel_asset_image_tasks("project-1", "character") == expected
    response = TestClient(app).post("/api/projects/project-2/assets/character/images/cancel")
    assert response.status_code == 202
    assert response.json()["asset_type"] == "character"
