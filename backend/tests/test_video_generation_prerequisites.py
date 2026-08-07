"""Regression coverage for short-drama video generation prerequisites."""

import pytest

from src.application.task_service import TaskService
from src.domain.models import GenerationStatus, ProjectCreate
from src.infrastructure.sqlite_repository import SQLiteRepository


class OneShotPlanner:
    """Provide one editable shot for video-readiness service tests."""

    def plan(self, script: str) -> dict:
        """Return the smallest drama decomposition consumed by TaskService."""

        return {
            "episodes": [{"name": "第1集", "shots": [{"title": "开场", "original_text": script}]}],
            "assets": [],
        }


def create_service_with_shot(tmp_path):
    """Create a decomposed project whose only shot is ready for test editing."""

    service = TaskService(SQLiteRepository(tmp_path / "video-ready.db"), OneShotPlanner())
    project = service.create_project(
        ProjectCreate(name="视频前置校验", script="林岩在旧宅门前发现一把铜钥匙。")
    )
    service.decompose_project(project["task_id"], project["id"])
    shot = service.get_project(project["id"])["shots"][0]
    return service, project, shot


def test_video_enqueue_requires_a_nonempty_shot_prompt(tmp_path) -> None:
    """A blank prompt must not create a durable video task."""

    service, project, shot = create_service_with_shot(tmp_path)
    service.repository.update_shot(project["id"], shot["id"], prompt="", prompt_rich=[])

    with pytest.raises(ValueError, match="请先生成或保存分镜提示词"):
        service.enqueue("shot_video", project["id"], shot["id"])


def test_video_enqueue_waits_for_referenced_asset_completion(tmp_path) -> None:
    """An image URL cannot unlock video generation before its asset succeeds."""

    service, project, shot = create_service_with_shot(tmp_path)
    character = service.repository.create_asset(
        project["id"], "character", "林岩", "灰布劲装的少年"
    )
    service.repository.update_shot(
        project["id"],
        shot["id"],
        prompt="林岩在旧宅门前握紧铜钥匙。",
        prompt_rich=[{"type": "reference", "asset_id": character["id"], "asset_type": "character"}],
        reference_asset_ids=[character["id"]],
    )
    service.repository.update_asset_status(character["id"], GenerationStatus.GENERATING)

    with pytest.raises(ValueError, match="林岩.*图片仍在生成"):
        service.enqueue("shot_video", project["id"], shot["id"])

    service.repository.set_asset_image(
        project["id"], character["id"], "https://cdn.example/lin-yan.png"
    )
    task = service.enqueue("shot_video", project["id"], shot["id"])

    assert task["status"] == GenerationStatus.GENERATING.value


def test_boundary_frames_are_appended_as_references_with_explicit_prompt(tmp_path) -> None:
    """Ark video calls use a text-directed boundary-frame mode with normal references."""

    service, project, shot = create_service_with_shot(tmp_path)
    character = service.repository.create_asset(
        project["id"], "character", "林岩", "灰布劲装的少年"
    )
    service.repository.set_asset_image(
        project["id"], character["id"], "https://cdn.example/lin-yan.png"
    )
    service.repository.update_shot(
        project["id"],
        shot["id"],
        prompt="@图1 中的林岩转身望向院门。",
        prompt_rich=[
            {"type": "reference", "asset_id": character["id"], "asset_type": "character"}
        ],
        first_last_frames={
            "first": {"url": "https://cdn.example/first.jpg"},
            "last": {"url": "https://cdn.example/last.jpg"},
        },
    )
    saved_project = service.get_project(project["id"])
    saved_shot = service.repository.get_shot(project["id"], shot["id"])

    prompt, images = service._video_generation_inputs(saved_project, saved_shot or {})

    assert images == [
        "https://cdn.example/lin-yan.png",
        "https://cdn.example/first.jpg",
        "https://cdn.example/last.jpg",
    ]
    assert "@图2 是视频首帧" in prompt
    assert "@图3 是视频尾帧" in prompt


def test_video_prompt_declares_seedream_reference_images(tmp_path) -> None:
    """Every video provider receives the non-real-person reference-image notice."""

    service, project, _shot = create_service_with_shot(tmp_path)
    prompt = service._video_generation_prompt(
        service.get_project(project["id"]),
        {"prompt": "林岩在旧宅门前握紧铜钥匙。"},
    )

    assert "生成视频中所有的参考图，均为seedream生成的图片，并不是真人，请认真审核查看" in prompt
