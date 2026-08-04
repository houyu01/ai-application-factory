import sqlite3

import pytest

from src.application.task_service import TaskService
from src.domain.models import GenerationStatus, ProjectCreate
from src.infrastructure.media_store import media_store
from src.infrastructure.sqlite_repository import SQLiteRepository
from src.llm_service.planner import ScriptPlanner


class FakePlanner:
    def plan(self, script: str) -> dict:
        return {
            "episodes": [
                {
                    "name": "第1集",
                    "shots": [
                        {
                            "id": "shot_001",
                            "title": "相遇",
                            "original_text": script,
                        }
                    ],
                }
            ],
            "assets": [
                {
                    "id": "char_001",
                    "type": "character",
                    "name": "小林",
                    "prompt": "年轻的主角",
                },
                {
                    "id": "scene_001",
                    "type": "scene",
                    "name": "车站",
                    "prompt": "黄昏的旧车站",
                },
                {
                    "id": "prop_001",
                    "type": "prop",
                    "name": "车票",
                    "prompt": "一张泛黄的车票",
                },
            ],
        }


def make_service(tmp_path) -> TaskService:
    service = TaskService(SQLiteRepository(tmp_path / "drama.db"), FakePlanner())
    service._probe_model_config = lambda config: None
    return service


def make_payload() -> ProjectCreate:
    return ProjectCreate(
        name="黄昏车站",
        script="小林在黄昏的车站捡到一张泛黄的车票。",
        video_public_prompt="整体保持电影化镜头语言，按剧本处理方式组织镜头。",
    )


def test_create_persists_empty_drama_and_decomposition_updates_all_resources(tmp_path):
    service = make_service(tmp_path)

    project = service.create_project(make_payload())

    assert project["status"] == GenerationStatus.GENERATING.value
    assert project["task"]["status"] == GenerationStatus.GENERATING.value
    assert service.get_project(project["id"])["shots"] == []
    assert "public_prompt" not in service.get_project(project["id"])
    assert service.get_project(project["id"])["video_public_prompt"].startswith("整体保持电影化")

    service.decompose_project(project["task_id"], project["id"])
    saved = service.get_project(project["id"])

    assert saved["status"] == GenerationStatus.SUCCEEDED.value
    assert saved["episodes"][0]["title"] == "第1集"
    assert len(saved["shots"]) == 1
    assert saved["shots"][0]["title"] == "相遇"
    assert saved["shots"][0]["original_text"]
    assert "小林" in saved["shots"][0]["prompt"]
    assert "车站" in saved["shots"][0]["prompt"]
    assert {asset["type"] for asset in saved["assets"]} == {"character", "scene", "prop"}
    assert saved["tasks"][0]["status"] == GenerationStatus.SUCCEEDED.value
    assert saved["tasks"][0]["trigger_type"] == "DRAMA_BOOTSTRAP"
    assert saved["tasks"][0]["input_snapshot"]["drama_id"] == project["id"]
    assert saved["tasks"][0]["result"]["shots"]
    listed = service.list_projects()[0]
    assert listed["episodes"] == [{"id": saved["episodes"][0]["id"], "sort_order": 1, "title": "第1集", "shot_count": 1}]
    assert listed["shots"][0]["id"] == saved["shots"][0]["id"]


def test_asset_prompt_starts_with_its_type_prompt(tmp_path):
    service = make_service(tmp_path)
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    asset = service.get_project(project["id"])["assets"][0]

    assert service._asset_generation_prompt(project, asset) == (
        "图片风格为「真人风格」，生成全身正视图以及一张面部特写（左边占二分之一的位置是超级大的"
        "正面面部特写，右边是二分之一放一张从头到鞋子的正视图，纯白背景，纯白背景）。\n\n年轻的主角"
    )


def test_voice_presets_are_seeded_and_character_voice_is_persisted(tmp_path):
    repository = SQLiteRepository(tmp_path / "voice.db")
    presets = repository.list_voice_presets()

    assert len(presets) >= 15
    assert presets[0]["id"] == "none"
    assert presets[1]["name"] == "破碎感低语坚韧音（女）"
    assert "耳语" in presets[1]["prompt"]

    service = TaskService(repository, FakePlanner())
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    saved = service.get_project(project["id"])
    character = next(asset for asset in saved["assets"] if asset["type"] == "character")

    updated = repository.update_asset(
        project["id"], character["id"], voice_id="warm_older_brother_male"
    )
    assert updated["voice_id"] == "warm_older_brother_male"
    cleared = repository.update_asset(project["id"], character["id"], voice_id="")
    assert cleared["voice_id"] is None
    updated = repository.update_asset(
        project["id"], character["id"], voice_id="warm_older_brother_male"
    )

    refreshed = service.get_project(project["id"])
    enriched = service._assets_with_voice_details(refreshed["assets"])
    enriched_character = next(asset for asset in enriched if asset["type"] == "character")
    assert enriched_character["voice_name"] == "温柔大哥哥音（男）"
    assert "温暖沉稳" in enriched_character["voice_prompt"]

    video_prompt = service._video_generation_prompt(
        refreshed, {"prompt": "角色在车站回头，轻声安慰同伴。"}
    )
    assert "温柔大哥哥音（男）" in video_prompt
    assert "温暖沉稳" in video_prompt


def test_asset_public_prompts_are_independent_and_persisted(tmp_path):
    service = make_service(tmp_path)
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])

    service.repository.update_asset_public_prompt(
        project["id"], "character", "角色统一正面设定，保持服装和面部特征一致。"
    )
    updated = service.repository.update_asset_public_prompt(
        project["id"], "scene", "场景统一电影级光线，保持空间结构连续。"
    )
    assets = {asset["type"]: asset for asset in updated["assets"]}

    assert updated["asset_public_prompts"] == {
        "character": "角色统一正面设定，保持服装和面部特征一致。",
        "scene": "场景统一电影级光线，保持空间结构连续。",
    }
    assert service._asset_generation_prompt(updated, assets["character"]).startswith(
        "角色统一正面设定，保持服装和面部特征一致。"
    )
    assert service._asset_generation_prompt(updated, assets["scene"]).startswith(
        "场景统一电影级光线，保持空间结构连续。"
    )
    assert service._asset_generation_prompt(updated, assets["prop"]).startswith(
        "图片风格为「真人风格」，主体道具清晰完整"
    )


def test_asset_variants_and_image_history_are_persisted(tmp_path):
    service = make_service(tmp_path)
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    character = next(
        asset
        for asset in service.get_project(project["id"])["assets"]
        if asset["type"] == "character"
    )

    created = service.repository.create_asset_variant(
        project["id"], character["id"], "青云山道袍", "保持面部特征一致，换穿青云山道袍。"
    )
    variant = created["variants"][0]
    updated = service.repository.update_asset_variant_status(
        project["id"], character["id"], variant["id"], GenerationStatus.SUCCEEDED,
        image_url="/api/media/variant.png",
    )
    saved_variant = updated["variants"][0]

    assert saved_variant["status"] == GenerationStatus.SUCCEEDED.value
    assert saved_variant["image_url"] == "/api/media/variant.png"
    assert saved_variant["image_history"][0]["url"] == "/api/media/variant.png"

    service.repository.update_asset_status(
        character["id"], GenerationStatus.SUCCEEDED, "/api/media/base.png"
    )
    saved_character = service.get_project(project["id"])["assets"][0]
    assert saved_character["image_history"][0]["url"] == "/api/media/base.png"


def test_video_public_prompt_can_be_updated_and_is_used_for_video_tasks(tmp_path):
    service = make_service(tmp_path)
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    updated = service.repository.update_video_public_prompt(
        project["id"], "统一视频镜头语言，保持节奏连贯。"
    )
    shot_id = service.get_project(project["id"])["shots"][0]["id"]
    for index, asset in enumerate(service.get_project(project["id"])["assets"], start=1):
        service.repository.set_asset_image(
            project["id"], asset["id"], f"https://cdn.example/video-{index}.png"
        )

    assert updated["video_public_prompt"] == "统一视频镜头语言，保持节奏连贯。"
    assert service._video_generation_prompt(
        updated, {"prompt": "角色沿着山路向前奔跑。"}
    ) == (
        "统一视频镜头语言，保持节奏连贯。\n\n"
        "分镜约束：不要字幕；不要背景音乐。\n\n"
        "角色沿着山路向前奔跑。"
    )
    task = service.enqueue("shot_video", project["id"], shot_id)
    service.run_shot_video(task["id"], project["id"], shot_id, "https://example.com/video.mp4")
    assert service.get_task(task["id"])["result"]["prompt"].startswith("统一视频镜头语言")


def test_inflight_generation_task_is_persisted_and_reused_after_refresh(tmp_path):
    service = make_service(tmp_path)
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    saved = service.get_project(project["id"])
    asset = saved["assets"][0]
    shot = saved["shots"][0]

    image_task = service.enqueue("asset_image", project["id"], asset["id"])
    repeated_image_task = service.enqueue("asset_image", project["id"], asset["id"])
    prompt_task = service.enqueue("shot_prompt", project["id"], shot["id"])
    refreshed = service.get_project(project["id"])

    assert repeated_image_task["id"] == image_task["id"]
    assert repeated_image_task["_reused"] is True
    assert image_task["_reused"] is False
    assert image_task["status"] == GenerationStatus.GENERATING.value
    assert prompt_task["status"] == GenerationStatus.GENERATING.value
    assert next(item for item in refreshed["assets"] if item["id"] == asset["id"])["status"] == GenerationStatus.GENERATING.value
    assert next(item for item in refreshed["shots"] if item["id"] == shot["id"])["status"] == GenerationStatus.GENERATING.value
    active_types = {(item["type"], item["resource_id"]) for item in refreshed["tasks"] if item["status"] == GenerationStatus.GENERATING.value}
    assert ("asset_image", asset["id"]) in active_types
    assert ("shot_prompt", shot["id"]) in active_types


def test_shot_prompt_generation_persists_rich_reference_nodes(tmp_path):
    service = TaskService(SQLiteRepository(tmp_path / "rich-prompt.db"), ScriptPlanner())
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    shot = service.get_project(project["id"])["shots"][0]

    task = service.enqueue("shot_prompt", project["id"], shot["id"])
    service.run_shot_prompt(task["id"], project["id"], shot["id"])
    saved_project = service.get_project(project["id"])
    saved = saved_project["shots"][0]

    references = [node for node in saved["prompt_rich"] if node["type"] == "reference"]
    assert references
    character_reference = next(
        node for node in references if node["asset_id"] == saved_project["assets"][0]["id"]
    )
    assert character_reference["mention_number"] >= 1
    assert f"@图{character_reference['mention_number']}（{saved_project['assets'][0]['name']}）" in saved["prompt"]
    assert service.get_task(task["id"])["result"]["prompt_rich"] == saved["prompt_rich"]
    quality_task = service.get_task(
        saved_project["tasks"][-1]["id"]
    )
    assert quality_task["type"] == "shot_quality"
    assert quality_task["status"] == GenerationStatus.GENERATING.value
    assert saved["quality_status"] == "检查中"


def test_shot_prompt_quality_issues_are_persisted_for_refresh(tmp_path):
    service = make_service(tmp_path)
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    shot = service.get_project(project["id"])["shots"][0]
    task = service.enqueue("shot_quality", project["id"], shot["id"])

    service.repository.update_shot(
        project["id"],
        shot["id"],
        prompt="",
        prompt_rich=[],
        quality_status="检查中",
        quality_issues=[],
    )
    service.run_shot_quality(task["id"], project["id"], shot["id"])

    saved = service.get_project(project["id"])["shots"][0]
    assert service.get_task(task["id"])["status"] == GenerationStatus.SUCCEEDED.value
    assert saved["quality_status"] == "需修改"
    assert saved["quality_issues"][0]["code"] == "EMPTY_PROMPT"


def test_model_probe_failure_does_not_persist_configuration(tmp_path):
    service = TaskService(SQLiteRepository(tmp_path / "probe.db"), FakePlanner())

    def fail_probe(config):
        raise ValueError("语言模型嗅探失败：连接被拒绝")

    service._probe_model_config = fail_probe
    with pytest.raises(ValueError, match="连接被拒绝"):
        service.save_model_config(
            {
                "kind": "language",
                "endpoint": "https://provider.example/v1",
                "api_key": "secret-key",
                "model": "provider-model",
                "models": ["provider-model"],
            }
        )

    assert service.repository.get_setting("language") is None


def test_drama_video_configuration_is_persisted(tmp_path):
    repository = SQLiteRepository(tmp_path / "drama-config.db")
    service = TaskService(repository, FakePlanner())
    project = service.create_project(
        ProjectCreate(
            name="配置短剧",
            script="主角在雨夜的车站发现一封没有署名的信。",
            ratio="16:9",
            style="2D动漫风",
            theme="悬疑",
            language_model="language-model",
            multimodal_model="image-model",
            video_model="video-model",
            resolution="480p",
            shot_constraints={"subtitles": True, "background_music": True},
        )
    )

    saved = service.get_project(project["id"])

    assert saved["ratio"] == "16:9"
    assert saved["style"] == "2D动漫风"
    assert saved["theme"] == "悬疑"
    assert saved["language_model"] == "language-model"
    assert saved["multimodal_model"] == "image-model"
    assert saved["video_model"] == "video-model"
    assert saved["resolution"] == "480p"
    assert saved["shot_constraints"] == {"subtitles": True, "background_music": True}


def test_shot_duration_is_persisted_within_the_supported_editor_range(tmp_path):
    service = make_service(tmp_path)
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    shot = service.get_project(project["id"])["shots"][0]

    updated = service.repository.update_shot(
        project["id"], shot["id"], duration_seconds=14
    )
    saved = service.get_project(project["id"])["shots"][0]

    assert updated["duration_seconds"] == 14
    assert saved["duration_seconds"] == 14
    assert saved["duration"] == 14


def test_model_endpoint_keeps_choices_and_project_selection_wins(tmp_path):
    service = make_service(tmp_path)
    service.save_model_config(
        {
            "kind": "language",
            "endpoint": "https://example.com/v1",
            "api_key": "secret-key-1234",
            "model": "provider-default",
            "models": ["provider-default", "provider-fast"],
        }
    )

    settings = service.get_model_configs()["language"]
    assert settings["endpoint"] == "https://example.com/v1"
    assert settings["model"] == "provider-default"
    assert settings["models"] == ["provider-default", "provider-fast"]
    assert settings["api_key_set"] is True
    assert "secret-key-1234" not in str(settings)

    project = service.create_project(make_payload())
    options = service._provider_options(service.get_project(project["id"]), "language")
    assert options["model"] == "provider-default"
    assert options["endpoint"] == "https://example.com/v1"

    updated = service.repository.update_model_selection(
        project["id"], {"language_model": "provider-fast"}
    )
    assert updated["language_model"] == "provider-fast"
    assert service._provider_options(updated, "language")["model"] == "provider-fast"


def test_multimodal_model_config_save_returns_public_config(tmp_path):
    service = make_service(tmp_path)
    saved = service.save_model_config(
        {
            "kind": "multimodal",
            "endpoint": "https://example.com/v1",
            "api_key": "image-secret-1234",
            "model": "provider-image",
            "models": ["provider-image", "provider-image-fast"],
        }
    )

    assert saved["kind"] == "multimodal"
    assert saved["model"] == "provider-image"
    assert saved["api_key_set"] is True
    assert "image-secret-1234" not in str(saved)


def test_video_task_urls_are_persisted_and_loaded_from_database(tmp_path):
    service = make_service(tmp_path)
    service.save_model_config(
        {
            "kind": "video",
            "create_url": "https://provider.example/create",
            "query_url": "https://provider.example/query/{id}",
            "api_key": "video-secret",
            "model": "provider-video",
            "models": ["provider-video"],
        }
    )

    stored = service.repository.get_setting("video")
    assert stored["create_url"] == "https://provider.example/create"
    assert stored["query_url"] == "https://provider.example/query/{id}"

    reloaded = make_service(tmp_path)
    config = reloaded.get_model_configs()["video"]
    assert config["create_url"] == stored["create_url"]
    assert config["query_url"] == stored["query_url"]
