"""Persistence coverage for project-level settings used before generation starts."""

import pytest

from src.application.task_service import TaskService
from src.domain.models import ProjectCreate
from src.infrastructure.sqlite_repository import SQLiteRepository


class InertPlanner:
    """Placeholder planner for settings tests that do not start decomposition."""


def make_service(tmp_path) -> TaskService:
    """Build an isolated service without invoking a language-model workflow."""

    service = TaskService(SQLiteRepository(tmp_path / "drama.db"), InertPlanner())
    service._probe_model_config = lambda _config: None
    return service


def test_project_persists_the_configured_expansion_character_range(tmp_path):
    """Creation settings must reach both the project row and durable task snapshot."""

    service = make_service(tmp_path)
    defaults = ProjectCreate(
        name="默认剧集数", script="小林在黄昏的车站捡到一张泛黄的车票。"
    )
    assert defaults.episode_count == 25
    assert defaults.expanded_script_min_chars == 5_000
    assert defaults.expanded_script_max_chars == 10_000
    assert defaults.shot_script_max_chars == 400
    default_project = service.create_project(defaults)
    default_task = service.repository.get_task(default_project["task_id"])
    assert default_task is not None
    assert default_task["input_snapshot"].get("episode_count") == 25
    assert default_task["input_snapshot"].get("expanded_script_min_chars") == 5_000
    assert default_task["input_snapshot"].get("expanded_script_max_chars") == 10_000
    assert default_task["input_snapshot"].get("shot_script_max_chars") == 400
    project = service.create_project(ProjectCreate(
        name="字数范围",
        script="小林在黄昏的车站捡到一张泛黄的车票。",
        episode_count=12,
        expanded_script_min_chars=1_000,
        expanded_script_max_chars=9_000,
        shot_script_max_chars=180,
    ))
    saved = service.get_project(project["id"])

    assert saved["expanded_script_min_chars"] == 1_000
    assert saved["expanded_script_max_chars"] == 9_000
    assert saved["episode_count"] == 12
    assert saved["shot_script_max_chars"] == 180
    task = service.repository.get_task(project["task_id"])
    assert task is not None
    assert task["input_snapshot"].get("expanded_script_min_chars") == 1_000
    assert task["input_snapshot"].get("expanded_script_max_chars") == 9_000
    assert task["input_snapshot"].get("episode_count") == 12
    assert task["input_snapshot"].get("shot_script_max_chars") == 180
    shots = service._flatten_shots(
        [{"name": "第1集", "shots": [{"title": "超长分镜", "original_text": "剧" * 240}]}],
        [],
        saved,
    )
    assert len(shots[0]["original_text"]) == 180
    with pytest.raises(ValueError, match="扩写字数最小值不能大于最大值"):
        ProjectCreate(
            name="无效范围",
            script="小林在黄昏的车站捡到一张泛黄的车票。",
            expanded_script_min_chars=80_000,
            expanded_script_max_chars=60_000,
        )
    with pytest.raises(ValueError, match="greater than or equal to 2"):
        ProjectCreate(name="剧集数过少", script="小林在黄昏的车站捡到一张泛黄的车票。", episode_count=1)
    with pytest.raises(ValueError, match="less than or equal to 100"):
        ProjectCreate(name="剧集数过多", script="小林在黄昏的车站捡到一张泛黄的车票。", episode_count=101)
    with pytest.raises(ValueError, match="greater than or equal to 1"):
        ProjectCreate(name="分镜上限过小", script="小林在黄昏的车站捡到一张泛黄的车票。", shot_script_max_chars=0)


def test_video_task_urls_are_persisted_and_loaded_from_database(tmp_path):
    """Video provider endpoints must survive a service restart."""

    service = make_service(tmp_path)
    service.save_model_config({
        "kind": "video",
        "create_url": "https://provider.example/create",
        "query_url": "https://provider.example/query/{id}",
        "api_key": "video-secret",
        "model": "provider-video",
        "models": ["provider-video"],
    })

    stored = service.repository.get_setting("video")
    assert stored["create_url"] == "https://provider.example/create"
    assert stored["query_url"] == "https://provider.example/query/{id}"

    reloaded = make_service(tmp_path)
    config = reloaded.get_model_configs()["video"]
    assert config["create_url"] == stored["create_url"]
    assert config["query_url"] == stored["query_url"]
