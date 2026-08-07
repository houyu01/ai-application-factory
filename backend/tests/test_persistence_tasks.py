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
    return TaskService(SQLiteRepository(tmp_path / "drama.db"), FakePlanner())


def make_payload() -> ProjectCreate:
    return ProjectCreate(
        name="黄昏车站",
        script="小林在黄昏的车站捡到一张泛黄的车票。",
        video_public_prompt="整体保持电影化镜头语言，按剧本处理方式组织镜头。",
    )


def mark_decomposition_assets_ready(service: TaskService, project_id: str) -> None:
    """Prepare every default rich-prompt dependency for a video-task test."""

    project = service.get_project(project_id)
    for index, asset in enumerate(project["assets"], start=1):
        service.repository.set_asset_image(
            project_id, asset["id"], f"https://cdn.example/{index}.png"
        )



def test_planner_ids_are_scoped_to_each_drama(tmp_path):
    service = make_service(tmp_path)
    first = service.create_project(make_payload())
    second = service.create_project(make_payload())

    service.decompose_project(first["task_id"], first["id"])
    service.decompose_project(second["task_id"], second["id"])

    first_saved = service.get_project(first["id"])
    second_saved = service.get_project(second["id"])
    assert first_saved["shots"][0]["id"] != second_saved["shots"][0]["id"]
    assert first_saved["assets"][0]["id"] != second_saved["assets"][0]["id"]


def test_decomposition_creates_rich_prompt_references_before_images(tmp_path):
    service = make_service(tmp_path)
    project = service.create_project(make_payload())

    service.decompose_project(project["task_id"], project["id"])
    saved = service.get_project(project["id"])
    shot = saved["shots"][0]
    references = [node for node in shot["prompt_rich"] if node["type"] == "reference"]

    assert shot["prompt"] != shot["original_text"]
    assert "场景：" in shot["prompt"]
    assert "\n角色：" in shot["prompt"]
    assert "【镜头1" in shot["prompt"]
    assert any("\n" in node.get("text", "") for node in shot["prompt_rich"])
    assert {node["asset_type"] for node in references} == {"character", "scene", "prop"}
    assert {node["asset_id"] for node in references} <= {
        asset["id"] for asset in saved["assets"]
    }
    assert all(not asset["image_url"] for asset in saved["assets"])


def test_decomposition_failure_is_persisted(tmp_path):
    class BrokenPlanner:
        def plan(self, script: str) -> dict:
            raise RuntimeError("planner unavailable")

    service = TaskService(SQLiteRepository(tmp_path / "broken.db"), BrokenPlanner())
    project = service.create_project(make_payload())

    service.decompose_project(project["task_id"], project["id"])
    saved = service.get_project(project["id"])

    assert saved["status"] == GenerationStatus.FAILED.value
    assert saved["tasks"][0]["status"] == GenerationStatus.FAILED.value
    assert saved["tasks"][0]["error_message"] == "planner unavailable"


def test_legacy_episode_table_is_migrated_and_episodes_are_aggregated(tmp_path):
    database_path = tmp_path / "legacy-episodes.db"
    repository = SQLiteRepository(database_path)
    project, _ = repository.create_drama_with_task(make_payload())

    with sqlite3.connect(database_path) as connection:
        connection.execute(
            "ALTER TABLE short_dramas ADD COLUMN episodes_json TEXT NOT NULL DEFAULT '[]'"
        )
        connection.execute(
            """
            CREATE TABLE drama_episodes (
                id TEXT PRIMARY KEY,
                drama_id TEXT NOT NULL,
                sort_order INTEGER NOT NULL,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            """
        )
        connection.execute(
            """
            INSERT INTO drama_episodes
                (id, drama_id, sort_order, title, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?)
            """,
            ("legacy-episode", project["id"], 3, "旧第3集", "now", "now"),
        )
        connection.execute(
            """
            INSERT INTO drama_shots (
                id, drama_id, episode_id, episode_name, shot_index, title,
                original_text, prompt, status, historical_videos_json,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                "legacy-shot",
                project["id"],
                "legacy-episode",
                "",
                1,
                "旧分镜",
                "旧文本",
                "旧提示词",
                GenerationStatus.NOT_GENERATED.value,
                "[]",
                "now",
                "now",
            ),
        )

    migrated_repository = SQLiteRepository(database_path)
    saved = migrated_repository.get_drama(project["id"])

    assert saved is not None
    assert saved["episodes"] == [
        {
            "id": "legacy-episode",
            "sort_order": 3,
            "title": "旧第3集",
            "shot_count": 1,
        }
    ]
    assert saved["shots"][0]["episode_sort_order"] == 3
    with sqlite3.connect(database_path) as connection:
        assert connection.execute(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'drama_episodes'"
        ).fetchone() is None
        assert "episodes_json" not in {
            row[1] for row in connection.execute("PRAGMA table_info(short_dramas)")
        }


def test_delete_project_removes_children_and_local_media(tmp_path, monkeypatch):
    media_root = tmp_path / "media"
    media_root.mkdir()
    monkeypatch.setattr(media_store, "root", media_root)
    media_store.configure({"provider": "local"})

    service = make_service(tmp_path)
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    saved = service.get_project(project["id"])
    asset = saved["assets"][0]
    shot = saved["shots"][0]
    media_url = media_store.save(b"generated-media", ".mp4")
    service.repository.update_asset_status(
        asset["id"], GenerationStatus.SUCCEEDED, media_url
    )
    service.repository.add_historical_video(
        project["id"],
        shot["id"],
        {"url": media_url, "generated_at": "2026-08-03T00:00:00Z"},
    )
    media_id = media_url.rsplit("/", 1)[-1]
    assert media_store.path_for(media_id) is not None

    result = service.delete_project(project["id"])

    assert result["status"] == "deleted"
    assert result["media_deleted"] == 1
    assert media_store.path_for(media_id) is None
    assert service.repository.get_drama(project["id"]) is None
    with sqlite3.connect(tmp_path / "drama.db") as connection:
        for table in ("short_dramas", "drama_assets", "drama_shots", "generation_tasks"):
            assert connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0] == 0


def test_shot_can_be_inserted_after_current_shot(tmp_path):
    service = make_service(tmp_path)
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    current = service.get_project(project["id"])["shots"][0]

    inserted = service.create_shot(project["id"], current["id"])
    saved = service.get_project(project["id"])

    assert inserted["original_text"] == ""
    assert inserted["prompt"] == ""
    assert [shot["id"] for shot in saved["shots"]] == [current["id"], inserted["id"]]
    assert [shot["shot_index"] for shot in saved["shots"]] == [1, 2]


def test_deleting_shot_cancels_active_video_task_and_removes_shot(tmp_path):
    service = make_service(tmp_path)
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    shot = service.get_project(project["id"])["shots"][0]
    mark_decomposition_assets_ready(service, project["id"])
    task = service.enqueue("shot_video", project["id"], shot["id"])

    result = service.delete_shot(project["id"], shot["id"])

    assert task["id"] in result["cancelled_task_ids"]
    assert result["next_shot_id"] is None
    assert service.repository.get_task(task["id"]) is None
    assert service.repository.get_shot(project["id"], shot["id"]) is None
    assert service.get_project(project["id"])["shots"] == []


def test_video_reference_images_are_extracted_from_rich_prompt_nodes(tmp_path):
    service = make_service(tmp_path)
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    saved = service.get_project(project["id"])
    character = saved["assets"][0]
    character["image_url"] = "https://example.com/character.png"
    scene = saved["assets"][1]
    scene["image_url"] = "https://example.com/scene.png"

    references = [
        {"type": "reference", "asset_id": character["id"]},
        {"type": "reference", "asset_id": scene["id"], "image_url": scene["image_url"]},
        {"type": "reference", "asset_id": character["id"]},
    ]
    assert service._video_reference_images(
        {"assets": [character, scene]}, {"prompt_rich": references}
    ) == ["https://example.com/character.png", "https://example.com/scene.png"]


def test_video_enqueue_rejects_missing_selected_reference_images(tmp_path):
    service = make_service(tmp_path)
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    saved = service.get_project(project["id"])
    shot = saved["shots"][0]
    character = saved["assets"][0]
    service.repository.update_shot(
        project["id"],
        shot["id"],
        prompt_rich=[
            {"type": "text", "text": "角色在车站回头。"},
            {"type": "reference", "asset_id": character["id"], "asset_type": "character"},
        ],
        reference_asset_ids=[character["id"]],
    )

    with pytest.raises(ValueError, match="小林.*图片未生成或未上传"):
        service.enqueue("shot_video", project["id"], shot["id"])

    service.repository.set_asset_image(
        project["id"], character["id"], "https://cdn.example/character.png"
    )
    task = service.enqueue("shot_video", project["id"], shot["id"])
    assert task["status"] == GenerationStatus.GENERATING.value


def test_cloud_local_media_origin_is_persisted_in_video_task(tmp_path):
    """A recovered worker keeps the public cloud origin used during enqueue."""

    service = make_service(tmp_path)
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    saved = service.get_project(project["id"])
    shot = saved["shots"][0]
    character = saved["assets"][0]
    service.repository.update_shot(
        project["id"], shot["id"],
        prompt_rich=[{"type": "reference", "asset_id": character["id"]}],
        reference_asset_ids=[character["id"]],
    )
    service.repository.set_asset_image(
        project["id"], character["id"], "/api/media/character.png"
    )

    with pytest.raises(ValueError, match="本地生成的图片无法调用大模型"):
        service.enqueue("shot_video", project["id"], shot["id"])
    task = service.enqueue(
        "shot_video", project["id"], shot["id"],
        public_media_base_url="https://studio.example.com",
    )

    assert task["input_snapshot"]["public_media_base_url"] == (
        "https://studio.example.com"
    )
    refreshed = service.get_project(project["id"])
    refreshed_shot = service.repository.get_shot(project["id"], shot["id"])
    assert service._video_reference_images(
        refreshed, refreshed_shot or {}, "https://studio.example.com"
    ) == ["https://studio.example.com/api/media/character.png"]


def test_shot_video_is_appended_to_shot_and_drama_history(tmp_path):
    service = make_service(tmp_path)
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    shot_id = service.get_project(project["id"])["shots"][0]["id"]
    mark_decomposition_assets_ready(service, project["id"])

    task = service.enqueue("shot_video", project["id"], shot_id)
    service.run_shot_video(task["id"], project["id"], shot_id, "https://example.com/video.mp4")
    saved = service.get_project(project["id"])

    assert service.get_task(task["id"])["status"] == GenerationStatus.SUCCEEDED.value
    assert service.get_task(task["id"])["result"]["prompt"].startswith("整体保持电影化")
    assert "小林" in service.get_task(task["id"])["result"]["prompt"]
    assert saved["shots"][0]["historical_videos"][0]["url"] == "https://example.com/video.mp4"
    assert saved["historical_videos"][0]["shot_id"] == shot_id


def test_unsupported_video_model_fails_task_shot_and_version(tmp_path, monkeypatch):
    """An Ark submission error must stop loading and survive a page refresh."""

    service = make_service(tmp_path)
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    shot_id = service.get_project(project["id"])["shots"][0]["id"]
    mark_decomposition_assets_ready(service, project["id"])
    task = service.enqueue("shot_video", project["id"], shot_id)
    monkeypatch.setattr(
        service,
        "_provider_options",
        lambda _project, _kind: {
            "api_key": "test-key",
            "endpoint": "https://ark.cn-beijing.volces.com/api/plan/v3",
            "create_url": "https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks",
            "query_url": "https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks/{id}",
            "model": "unsupported-video-model",
        },
    )

    def reject_model(*_args, **_kwargs):
        raise RuntimeError(
            'Ark API 请求失败（HTTP 404）：{"error":{"code":"UnsupportedModel",'
            '"message":"The requested model does not support the agent plan feature."}}'
        )

    monkeypatch.setattr(
        "src.application.task_service_worker_mixin.ArkClient.create_video_task",
        reject_model,
    )
    service.advance_shot_video_task(service.get_task(task["id"]))

    saved = service.get_project(project["id"])
    saved_task = service.get_task(task["id"])
    saved_shot = saved["shots"][0]
    assert saved_task["status"] == GenerationStatus.FAILED.value
    assert saved_shot["status"] == GenerationStatus.FAILED.value
    assert saved_shot["versions"][0]["status"] == GenerationStatus.FAILED.value
    assert "全局参数 → 视频模型" in saved_task["error_message"]
    assert "unsupported-video-model" in saved_shot["versions"][0]["error_message"]


def test_sensitive_boundary_frame_fails_task_with_replacement_guidance(tmp_path, monkeypatch):
    """Ark privacy moderation must become a recoverable, user-facing task error."""

    service = make_service(tmp_path)
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    shot_id = service.get_project(project["id"])["shots"][0]["id"]
    mark_decomposition_assets_ready(service, project["id"])
    task = service.enqueue("shot_video", project["id"], shot_id)
    monkeypatch.setattr(
        service,
        "_provider_options",
        lambda _project, _kind: {
            "api_key": "test-key",
            "endpoint": "https://ark.cn-beijing.volces.com/api/plan/v3",
            "create_url": "https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks",
            "query_url": "https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks/{id}",
            "model": "doubao-seedance-2.0",
        },
    )
    monkeypatch.setattr(
        "src.application.task_service_worker_mixin.ArkClient.create_video_task",
        lambda *_args, **_kwargs: (_ for _ in ()).throw(RuntimeError(
            'Ark API 请求失败（HTTP 400）：{"error":{"code":"InputImageSensitiveContentDetected.PrivacyInformation"}}'
        )),
    )

    service.advance_shot_video_task(service.get_task(task["id"]))

    assert "更换为不含真实人物面部" in service.get_task(task["id"])["error_message"]


def test_long_shot_prompt_template_is_selected_and_snapshotted(tmp_path):
    """A shot's selected mode must survive queueing and produce one long camera take."""

    service = TaskService(SQLiteRepository(tmp_path / "long-shot.db"), ScriptPlanner())
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    shot = service.get_project(project["id"])["shots"][0]
    service.repository.update_shot(
        project["id"], shot["id"], prompt_template_version="v2"
    )

    task = service.enqueue("shot_prompt", project["id"], shot["id"])
    service.run_shot_prompt(task["id"], project["id"], shot["id"])
    saved = service.get_project(project["id"])["shots"][0]

    assert task["input_snapshot"]["prompt_template_version"] == "v2"
    assert saved["prompt_template_version"] == "v2"
    assert saved["prompt"].count("【镜头") == 1
