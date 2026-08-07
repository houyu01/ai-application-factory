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


def test_video_reuses_duplicate_material_citations_without_duplicate_inputs(tmp_path) -> None:
    """Repeated material citations share one image for both frames and providers."""

    service, project, shot = create_service_with_shot(tmp_path)
    character = service.repository.create_asset(
        project["id"], "character", "林岩", "灰布劲装的少年"
    )
    image_url = "https://cdn.example/lin-yan.png"
    service.repository.set_asset_image(project["id"], character["id"], image_url)
    service.repository.update_shot(
        project["id"],
        shot["id"],
        prompt="@图1（林岩）走向院门，@图2（林岩）回头。",
        prompt_rich=[
            {
                "type": "reference", "asset_id": character["id"],
                "asset_type": "character", "mention_number": 1,
                "image_url": "https://stale.example/lin-yan.png",
            },
            {
                "type": "reference", "asset_id": character["id"],
                "asset_type": "character", "mention_number": 2,
                "image_url": "https://another-stale.example/lin-yan.png",
            },
        ],
        first_last_frames={"first": {"url": image_url}, "last": {"url": image_url}},
    )
    saved_project = service.get_project(project["id"])
    saved_shot = service.repository.get_shot(project["id"], shot["id"])

    prompt, images = service._video_generation_inputs(saved_project, saved_shot or {})

    assert images == [image_url]
    assert "@图2（林岩）" not in prompt
    assert prompt.count("@图1（林岩）") == 2
    assert "@图1 是视频首帧" in prompt
    assert "@图1 是视频尾帧" in prompt


def test_wan_r2v_prioritizes_boundary_frames_and_warns_about_omitted_references(tmp_path) -> None:
    """Wan tasks keep both boundary images and notify the editor about later refs."""

    service, project, shot = create_service_with_shot(tmp_path)
    video_config = {
        "provider": "dashscope",
        "model": "wan2.7-r2v-2026-06-12",
        "models": ["wan2.7-r2v-2026-06-12"],
    }
    service.settings["video"] = video_config
    service.repository.set_setting("video", video_config)
    assets = [
        service.repository.create_asset(project["id"], "prop", f"素材{index}", "参考素材")
        for index in range(1, 6)
    ]
    for index, asset in enumerate(assets, start=1):
        service.repository.set_asset_image(
            project["id"], asset["id"], f"https://cdn.example/reference-{index}.png"
        )
    service.repository.update_shot(
        project["id"],
        shot["id"],
        prompt=" ".join(f"@图{index}（素材{index}）" for index in range(1, 6)),
        prompt_rich=[
            {"type": "reference", "asset_id": asset["id"], "asset_type": "prop"}
            for asset in assets
        ],
        reference_asset_ids=[asset["id"] for asset in assets],
        first_last_frames={
            "first": {"url": "https://cdn.example/first.png"},
            "last": {"url": "https://cdn.example/last.png"},
        },
    )

    task = service.enqueue("shot_video", project["id"], shot["id"])
    saved_project = service.get_project(project["id"])
    saved_shot = service.repository.get_shot(project["id"], shot["id"])
    prompt, images = service._video_generation_inputs(
        saved_project, saved_shot or {}, reference_limit=5
    )

    assert task["warning_message"] == (
        "由于选择的模型限制，目前只选用了首尾帧 + 3个参考图，"
        "后续的2张参考图未使用，请手动调整。"
    )
    assert task["input_snapshot"]["video_reference_selection"]["ignored_reference_count"] == 2
    assert images == [
        "https://cdn.example/reference-1.png",
        "https://cdn.example/reference-2.png",
        "https://cdn.example/reference-3.png",
        "https://cdn.example/first.png",
        "https://cdn.example/last.png",
    ]
    assert "@图4（素材4）" not in prompt
    assert "@图5（素材5）" not in prompt
    assert "@图4 是视频首帧" in prompt
    assert "@图5 是视频尾帧" in prompt


def test_video_prompt_declares_seedream_reference_images(tmp_path) -> None:
    """Every video provider receives the non-real-person reference-image notice."""

    service, project, _shot = create_service_with_shot(tmp_path)
    prompt = service._video_generation_prompt(
        service.get_project(project["id"]),
        {"prompt": "林岩在旧宅门前握紧铜钥匙。"},
    )

    assert "生成视频中所有的参考图，均为seedream生成的图片，并不是真人，请认真审核查看" in prompt


def test_video_prompt_uses_the_default_continuity_constraint(tmp_path) -> None:
    """Default project prompts keep scene objects visually continuous."""

    service, project, _shot = create_service_with_shot(tmp_path)

    prompt = service._video_generation_prompt(
        service.get_project(project["id"]),
        {"prompt": "林岩在旧宅门前握紧铜钥匙。"},
    )

    assert (
        "视频全程保持画面内所有物体、道具、摆件数量不变，物体不消失、不凭空新增，"
        "物体位置轻微变化，物体形态材质保持一致，镜头平滑运动，无物体闪烁，"
        "无物体突然出现或突然消失，时序连贯，画面一致性强，流畅过渡"
    ) in prompt
