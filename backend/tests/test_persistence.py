import sqlite3

from src.application.task_service import TaskService
from src.domain.models import GenerationStatus, ProjectCreate
from src.infrastructure.sqlite_repository import SQLiteRepository


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
    return ProjectCreate(name="黄昏车站", script="小林在黄昏的车站捡到一张泛黄的车票。")


def test_create_persists_empty_drama_and_decomposition_updates_all_resources(tmp_path):
    service = make_service(tmp_path)

    project = service.create_project(make_payload())

    assert project["status"] == GenerationStatus.GENERATING.value
    assert project["task"]["status"] == GenerationStatus.GENERATING.value
    assert service.get_project(project["id"])["shots"] == []

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


def test_shot_video_is_appended_to_shot_and_drama_history(tmp_path):
    service = make_service(tmp_path)
    project = service.create_project(make_payload())
    service.decompose_project(project["task_id"], project["id"])
    shot_id = service.get_project(project["id"])["shots"][0]["id"]

    task = service.enqueue("shot_video", project["id"], shot_id)
    service.run_shot_video(task["id"], project["id"], shot_id, "https://example.com/video.mp4")
    saved = service.get_project(project["id"])

    assert service.get_task(task["id"])["status"] == GenerationStatus.SUCCEEDED.value
    assert saved["shots"][0]["historical_videos"][0]["url"] == "https://example.com/video.mp4"
    assert saved["historical_videos"][0]["shot_id"] == shot_id


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
