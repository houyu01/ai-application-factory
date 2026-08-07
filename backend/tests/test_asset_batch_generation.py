"""Regression coverage for five-at-a-time Generate All asset images."""

from fastapi.testclient import TestClient

from src.api import asset_batch_routes
from src.application.task_service import TaskService
from src.domain.models import GenerationStatus, ProjectCreate
from src.infrastructure.sqlite_repository import SQLiteRepository
from src.main import app


class BatchPlanner:
    """Provide one initial character so batch tests can add a stable ordered list."""

    def plan(self, script: str) -> dict:
        return {
            "episodes": [{"name": "第1集", "shots": [{"title": "开场", "original_text": script}]}],
            "assets": [{"type": "character", "name": "角色 1", "prompt": "角色设定"}],
        }


def make_service(tmp_path) -> TaskService:
    """Create an isolated service with a completed project ready for asset tasks."""

    service = TaskService(SQLiteRepository(tmp_path / "asset-batch.db"), BatchPlanner())
    project = service.create_project(ProjectCreate(name="批次短剧", script="这是可用于分批生成素材的测试剧本内容。"))
    service.decompose_project(project["task_id"], project["id"])
    return service


def test_asset_image_batch_starts_only_five_jobs_and_continues_in_order(tmp_path, monkeypatch) -> None:
    """Generate All must wait for a five-image batch before enqueuing the next one."""

    service = make_service(tmp_path)
    project = service.list_projects()[0]
    first_asset = service.get_project(project["id"])["assets"][0]
    assets = [first_asset]
    for index in range(2, 7):
        assets.append(
            service.repository.create_asset(
                project["id"], "character", f"角色 {index}", "角色设定"
            )
        )
    asset_ids = [asset["id"] for asset in assets]

    batch = service.enqueue_asset_image_batch(project["id"], asset_ids)
    repeated = service.enqueue_asset_image_batch(project["id"], asset_ids)

    assert batch["status"] == GenerationStatus.GENERATING.value
    assert repeated["id"] == batch["id"]
    assert repeated["_reused"] is True
    assert all(asset["status"] == GenerationStatus.NOT_GENERATED.value for asset in assets)

    service.resume_task(service.repository.get_task(batch["id"]) or {})
    first_snapshot = service.repository.get_task(batch["id"])["input_snapshot"]
    first_children = [
        service.repository.get_task(task_id)
        for task_id in first_snapshot["active_task_ids"]
    ]

    assert first_snapshot["next_index"] == 5
    assert [task["resource_id"] for task in first_children] == asset_ids[:5]
    assert all(task["status"] == GenerationStatus.GENERATING.value for task in first_children)
    assert service.repository.get_asset(project["id"], asset_ids[5])["status"] == GenerationStatus.NOT_GENERATED.value

    monkeypatch.setattr(service, "_generate_image_url", lambda _project, asset: f"https://images.test/{asset['id']}.png")
    for child in first_children:
        service.resume_task(child)
    service.resume_task(service.repository.get_task(batch["id"]) or {})

    second_snapshot = service.repository.get_task(batch["id"])["input_snapshot"]
    second_child = service.repository.get_task(second_snapshot["active_task_ids"][0])
    assert second_snapshot["next_index"] == 6
    assert second_child["resource_id"] == asset_ids[5]
    assert second_child["status"] == GenerationStatus.GENERATING.value

    service.resume_task(second_child)
    service.resume_task(service.repository.get_task(batch["id"]) or {})
    completed = service.repository.get_task(batch["id"])

    assert completed["status"] == GenerationStatus.SUCCEEDED.value
    assert completed["result"] == {
        "total_count": 6,
        "completed_count": 6,
        "failed_count": 0,
        "cancelled_count": 0,
    }


def test_asset_image_batch_endpoint_delegates_to_the_durable_service(monkeypatch) -> None:
    """The asset drawer must receive a 202 task rather than submit every image itself."""

    called: dict[str, object] = {}

    def enqueue(project_id: str, asset_ids: list[str]) -> dict[str, object]:
        called.update(project_id=project_id, asset_ids=asset_ids)
        return {
            "id": "asset-batch-1",
            "type": "asset_image_batch",
            "status": "生成中",
            "project_id": project_id,
            "resource_id": "character",
            "created_at": "2026-08-06T00:00:00Z",
            "progress": 0,
            "stage": "",
        }

    monkeypatch.setattr(asset_batch_routes.task_service, "enqueue_asset_image_batch", enqueue)

    response = TestClient(app).post(
        "/api/projects/project-1/assets/images/batch",
        json={"asset_ids": ["asset-1", "asset-2"]},
    )

    assert response.status_code == 202
    assert response.json()["id"] == "asset-batch-1"


def test_reference_image_batch_only_enqueues_selected_assets_without_images(tmp_path, monkeypatch) -> None:
    """One-click reference generation must leave selected ready images untouched."""

    service = make_service(tmp_path)
    project = service.get_project(service.list_projects()[0]["id"])
    shot = project["shots"][0]
    ready_character = project["assets"][0]
    missing_character = service.repository.create_asset(
        project["id"], "character", "待生成角色", "角色设定"
    )
    missing_scene = service.repository.create_asset(
        project["id"], "scene", "待生成场景", "场景设定"
    )
    service.repository.set_asset_image(
        project["id"], ready_character["id"], "https://images.test/ready.png"
    )
    service.repository.update_shot(
        project["id"],
        shot["id"],
        prompt_rich=[
            {"type": "reference", "asset_id": ready_character["id"]},
            {"type": "reference", "asset_id": missing_character["id"]},
            {"type": "reference", "asset_id": missing_scene["id"]},
            {"type": "reference", "asset_id": missing_character["id"]},
        ],
        reference_asset_ids=[
            ready_character["id"], missing_character["id"], missing_scene["id"]
        ],
    )

    batch = service.enqueue_missing_shot_reference_images(project["id"], shot["id"])
    snapshot = batch["input_snapshot"]

    assert batch["type"] == "shot_reference_image_batch"
    assert batch["resource_id"] == shot["id"]
    assert snapshot["asset_ids"] == [missing_character["id"], missing_scene["id"]]
    assert snapshot["reference_asset_ids"] == [
        ready_character["id"], missing_character["id"], missing_scene["id"]
    ]

    service.resume_task(service.repository.get_task(batch["id"]) or {})
    child_tasks = [
        service.repository.get_task(task_id)
        for task_id in service.repository.get_task(batch["id"])["input_snapshot"]["active_task_ids"]
    ]
    assert [task["resource_id"] for task in child_tasks] == [
        missing_character["id"], missing_scene["id"]
    ]
    assert service.repository.get_asset(project["id"], ready_character["id"])["image_url"] == "https://images.test/ready.png"

    monkeypatch.setattr(
        service,
        "_generate_image_url",
        lambda _project, asset: f"https://images.test/{asset['id']}.png",
    )
    for child in child_tasks:
        service.resume_task(child)
    service.resume_task(service.repository.get_task(batch["id"]) or {})

    completed = service.repository.get_task(batch["id"])
    assert completed["status"] == GenerationStatus.SUCCEEDED.value
    assert completed["result"]["total_count"] == 2


def test_reference_image_batch_endpoint_delegates_to_the_durable_service(monkeypatch) -> None:
    """The warning action must return a durable task for its selected shot."""

    called: dict[str, str] = {}

    def enqueue(project_id: str, shot_id: str) -> dict[str, object]:
        called.update(project_id=project_id, shot_id=shot_id)
        return {
            "id": "reference-batch-1",
            "type": "shot_reference_image_batch",
            "status": "生成中",
            "project_id": project_id,
            "resource_id": shot_id,
            "created_at": "2026-08-06T00:00:00Z",
            "progress": 0,
            "stage": "",
        }

    monkeypatch.setattr(asset_batch_routes.task_service, "enqueue_missing_shot_reference_images", enqueue)

    response = TestClient(app).post(
        "/api/projects/project-1/shots/shot-1/reference-images/generate"
    )

    assert response.status_code == 202
    assert response.json()["id"] == "reference-batch-1"
    assert called == {"project_id": "project-1", "shot_id": "shot-1"}
    assert called == {"project_id": "project-1", "asset_ids": ["asset-1", "asset-2"]}
