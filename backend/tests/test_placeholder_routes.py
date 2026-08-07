"""Regression coverage for the shot placeholder draft and generation APIs."""

from io import BytesIO

from fastapi.testclient import TestClient
from PIL import Image

from src.api import placeholder_routes
from src.application.task_service import TaskService
from src.domain.models import (
    DramaPlaceholderLayoutUpdate,
    DramaPlaceholderPlacement,
    GenerationStatus,
    ProjectCreate,
)
from src.infrastructure.media_store import media_store
from src.infrastructure.sqlite_repository import SQLiteRepository
from src.llm_service.client.ark_client import ArkClient
from src.main import app


def _payload() -> dict:
    """Return one valid scene and character layout submitted by the dialog."""

    return {
        "shot_id": "shot-1",
        "scene_asset_id": "scene-1",
        "placements": [
            {
                "asset_id": "character-1",
                "x": 0.1,
                "y": 0.2,
                "width": 0.25,
                "height": 0.4,
                "note": "画面左侧站立",
            }
        ],
    }


def test_save_placeholder_layout_persists_the_editor_draft(monkeypatch) -> None:
    """Saving the modal draft must update only the shot selected in the URL."""

    saved: dict = {}
    payload = DramaPlaceholderLayoutUpdate(
        shot_id="shot-1",
        scene_asset_id="scene-1",
        placements=[DramaPlaceholderPlacement(**_payload()["placements"][0])],
    )

    def update_shot(project_id: str, shot_id: str, **fields: object) -> dict:
        saved.update(project_id=project_id, shot_id=shot_id, **fields)
        return {"id": shot_id, **fields}

    monkeypatch.setattr(
        placeholder_routes.task_service.repository, "update_shot", update_shot
    )

    result = placeholder_routes.save_placeholder_layout("project-1", "shot-1", payload)

    assert result["id"] == "shot-1"
    assert saved["placeholder_scene_asset_id"] == "scene-1"
    assert saved["placeholder_placements"][0]["asset_id"] == "character-1"


def test_generate_placeholder_image_is_exposed_over_http(monkeypatch) -> None:
    """The Generate Placeholder Image button must receive a durable 202 task."""

    called: dict = {}

    def enqueue(
        project_id: str,
        shot_id: str,
        scene_asset_id: str,
        placements: list[dict[str, object]],
    ) -> dict:
        called.update(
            project_id=project_id,
            shot_id=shot_id,
            scene_asset_id=scene_asset_id,
            placements=placements,
        )
        return {
            "id": "placeholder-task-1",
            "type": "placeholder_image",
            "status": "生成中",
            "project_id": project_id,
            "resource_id": "placeholder-asset-1",
            "created_at": "2026-08-04T00:00:00Z",
            "progress": 0,
            "stage": "",
        }

    monkeypatch.setattr(placeholder_routes.task_service, "enqueue_placeholder_image", enqueue)

    response = TestClient(app).post(
        "/api/projects/project-1/placeholders/image", json=_payload()
    )

    assert response.status_code == 202
    assert response.json()["id"] == "placeholder-task-1"
    assert called["scene_asset_id"] == "scene-1"
    assert called["placements"][0]["asset_id"] == "character-1"


class _PlaceholderPlanner:
    """Provide scene, character, and prop assets for clean placeholder generation."""

    def plan(self, script: str) -> dict:
        return {
            "episodes": [{"name": "第1集", "shots": [{"title": "相遇", "original_text": script}]}],
            "assets": [
                {"type": "character", "name": "林岩", "prompt": "青年剑修"},
                {"type": "scene", "name": "山门前", "prompt": "清晨的山门"},
                {"type": "prop", "name": "木剑", "prompt": "磨损的古朴木剑"},
            ],
        }


def _image_bytes(color: tuple[int, int, int]) -> bytes:
    """Create a tiny valid image returned by the mocked image provider."""

    image = Image.new("RGB", (80, 60), color)
    output = BytesIO()
    image.save(output, format="PNG")
    return output.getvalue()


def test_placeholder_task_generates_clean_composite_and_adds_reference(
    tmp_path, monkeypatch
) -> None:
    """The worker must generate a clean composite and persist it as a new version."""

    media_root = tmp_path / "media"
    media_root.mkdir()
    monkeypatch.setattr(media_store, "root", media_root)
    media_store.configure({"provider": "local"})
    service = TaskService(SQLiteRepository(tmp_path / "drama.db"), _PlaceholderPlanner())
    service.settings["multimodal"] = {
        "api_key": "test-key",
        "endpoint": "https://ark.cn-beijing.volces.com/api/plan/v3",
        "model": "doubao-seeddream-test",
    }
    project = service.create_project(
        ProjectCreate(name="山门相遇", script="林岩带着木剑来到山门前，准备拜见师父。")
    )
    service.decompose_project(project["task_id"], project["id"])
    saved = service.get_project(project["id"])
    character = next(asset for asset in saved["assets"] if asset["type"] == "character")
    scene = next(asset for asset in saved["assets"] if asset["type"] == "scene")
    prop = next(asset for asset in saved["assets"] if asset["type"] == "prop")
    service.repository.set_asset_image(
        project["id"], character["id"], media_store.save(_image_bytes((210, 180, 150)), ".png")
    )
    service.repository.set_asset_image(
        project["id"], scene["id"], media_store.save(_image_bytes((80, 130, 180)), ".png")
    )
    service.repository.set_asset_image(
        project["id"], prop["id"], media_store.save(_image_bytes((120, 90, 60)), ".png")
    )
    generated: dict[str, object] = {}

    def generate_image(
        _client: ArkClient,
        prompt: str,
        *,
        reference_images: list[str] | None = None,
        size: str = "2K",
    ) -> dict[str, object]:
        generated.update(prompt=prompt, reference_images=reference_images, size=size)
        return {
            "content": _image_bytes((30, 70, 110)),
            "content_type": "image/png",
        }

    monkeypatch.setattr(ArkClient, "generate_image", generate_image)

    shot = service.get_project(project["id"])["shots"][0]
    task = service.enqueue_placeholder_image(
        project["id"],
        shot["id"],
        scene["id"],
        [{"asset_id": character["id"], "x": 0.2, "y": 0.25, "width": 0.25, "height": 0.45}],
    )
    service.run_placeholder_image(task["id"], project["id"], task["resource_id"])

    completed = service.repository.get_task(task["id"])
    updated_shot = service.repository.get_shot(project["id"], shot["id"])
    placeholder = service.repository.get_asset(project["id"], task["resource_id"])
    assert completed["status"] == GenerationStatus.SUCCEEDED.value
    assert placeholder["image_url"].startswith("/api/media/")
    assert placeholder["metadata"]["render_mode"] == "generated_composite"
    assert placeholder["metadata"]["version"] == 1
    assert placeholder["metadata"]["reference_asset_ids"] == [
        scene["id"],
        character["id"],
        prop["id"],
    ]
    assert len(generated["reference_images"]) == 3
    assert "无编辑痕迹" in str(generated["prompt"])
    assert "不要方框" in str(generated["prompt"])
    assert any(node.get("asset_id") == placeholder["id"] for node in updated_shot["prompt_rich"])


def test_video_references_ignore_legacy_boxed_placeholder() -> None:
    """Video input must ignore editor exports and use clean generated placeholders."""

    project = {
        "assets": [
            {
                "id": "legacy-placeholder",
                "type": "placeholder",
                "image_url": "https://example.com/boxed.png",
                "metadata": {"shot_id": "shot-1"},
            },
            {
                "id": "clean-placeholder",
                "type": "placeholder",
                "image_url": "https://example.com/clean.png",
                "metadata": {"render_mode": "generated_composite"},
            },
        ]
    }
    shot = {
        "prompt_rich": [
            {
                "type": "reference",
                "asset_id": "legacy-placeholder",
                "asset_type": "placeholder",
            },
            {
                "type": "reference",
                "asset_id": "clean-placeholder",
                "asset_type": "placeholder",
            },
        ]
    }

    assert TaskService._video_reference_images(project, shot) == [
        "https://example.com/clean.png"
    ]
