"""Regression coverage for durable short-drama cover generation."""

from src.application.task_service import TaskService
from src.domain.models import GenerationStatus, ProjectCreate
from src.infrastructure.sqlite_repository import SQLiteRepository


class CoverPlanner:
    """Return one shot with reusable character and scene cover references."""

    def plan(self, script: str) -> dict:
        return {
            "episodes": [
                {
                    "name": "第1集",
                    "shots": [
                        {"id": "shot_001", "title": "开场", "original_text": script}
                    ],
                }
            ],
            "assets": [
                {
                    "id": "char_001",
                    "type": "character",
                    "name": "林岩",
                    "prompt": "青年剑修",
                },
                {
                    "id": "scene_001",
                    "type": "scene",
                    "name": "山村旧居",
                    "prompt": "阴云下的旧村落",
                },
            ],
        }


def cover_service(tmp_path) -> TaskService:
    """Create an isolated service whose persisted tasks can be resumed."""

    return TaskService(SQLiteRepository(tmp_path / "cover.db"), CoverPlanner())


def prepared_cover_project(service: TaskService) -> tuple[dict, dict, dict, dict]:
    """Create one project and mark all selected reference images ready."""

    project = service.create_project(
        ProjectCreate(name="御剑望云", script="林岩回到旧居寻找故乡被毁的真相。")
    )
    service.decompose_project(project["task_id"], project["id"])
    saved = service.get_project(project["id"])
    character = next(item for item in saved["assets"] if item["type"] == "character")
    scene = next(item for item in saved["assets"] if item["type"] == "scene")
    service.repository.set_asset_image(
        project["id"], character["id"], "https://cdn.example/lin-yan.png"
    )
    service.repository.set_asset_image(
        project["id"], scene["id"], "https://cdn.example/village.png"
    )
    upload = service.repository.create_asset(
        project["id"], "cover_reference", "构图参考", "用户上传的封面构图"
    )
    service.repository.set_asset_image(
        project["id"], upload["id"], "https://cdn.example/layout.png"
    )
    return project, character, scene, upload


def test_cover_task_generates_requested_history_and_is_restart_idempotent(
    tmp_path, monkeypatch
):
    service = cover_service(tmp_path)
    project, character, scene, upload = prepared_cover_project(service)
    generated: list[str] = []

    def fake_generate(_project, _cover, _references):
        url = f"https://cdn.example/cover-{len(generated) + 1}.png"
        generated.append(url)
        return url

    monkeypatch.setattr(service, "_generate_cover_url", fake_generate)
    queued = service.enqueue_cover_image(
        project["id"],
        name="御剑望云封面",
        prompt="突出人物与故乡废墟",
        ratio="9:16",
        count=3,
        character_asset_ids=[character["id"]],
        scene_asset_ids=[scene["id"]],
        extra_reference_asset_ids=[upload["id"]],
    )

    assert queued["task"]["status"] == GenerationStatus.GENERATING.value
    service.resume_task(service.repository.get_task(queued["task"]["id"]))

    cover = service.repository.get_asset(project["id"], queued["cover"]["id"])
    task = service.repository.get_task(queued["task"]["id"])
    assert cover["status"] == GenerationStatus.SUCCEEDED.value
    assert [item["url"] for item in cover["image_history"]] == generated
    assert len(generated) == 3
    assert task["status"] == GenerationStatus.SUCCEEDED.value
    assert task["result"]["image_urls"] == generated

    service.run_cover_image(task["id"], project["id"], cover["id"])
    assert len(generated) == 3


def test_cover_generation_rejects_reference_without_image(tmp_path):
    service = cover_service(tmp_path)
    project = service.create_project(
        ProjectCreate(name="未就绪素材", script="林岩回到村庄，发现一切已经改变。")
    )
    service.decompose_project(project["task_id"], project["id"])
    character = next(
        item for item in service.get_project(project["id"])["assets"]
        if item["type"] == "character"
    )

    try:
        service.enqueue_cover_image(
            project["id"], name="封面", prompt="", ratio="9:16", count=1,
            character_asset_ids=[character["id"]], scene_asset_ids=[],
            extra_reference_asset_ids=[],
        )
    except ValueError as exc:
        assert "请先生成或上传" in str(exc)
    else:
        raise AssertionError("Cover generation should reject a missing reference image")
