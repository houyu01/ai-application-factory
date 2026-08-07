import pytest

from src.application.task_service import TaskService
from src.infrastructure.sqlite_repository import SQLiteRepository


def make_service(tmp_path) -> TaskService:
    return TaskService(SQLiteRepository(tmp_path / "model-settings.db"), object())


def test_deleted_video_model_option_stays_deleted_after_reload(tmp_path):
    service = make_service(tmp_path)
    service.save_model_options(
        "video", ["doubao-seedance-2.0", "sora-2"], "doubao-seedance-2.0"
    )

    saved = service.save_model_options(
        "video", ["doubao-seedance-2.0"], "doubao-seedance-2.0"
    )
    reloaded = make_service(tmp_path).get_model_configs()["video"]

    assert saved["models"] == ["doubao-seedance-2.0"]
    assert reloaded["models"] == ["doubao-seedance-2.0"]
    assert "sora-2" not in reloaded["models"]


def test_explicit_empty_model_options_do_not_restore_defaults(tmp_path):
    service = make_service(tmp_path)

    service.save_model_options("video", [], "")
    reloaded = make_service(tmp_path).get_model_configs()["video"]

    assert reloaded["model"] == ""
    assert reloaded["models"] == []


def test_probe_failure_names_the_actual_model(tmp_path):
    service = make_service(tmp_path)

    def fail_video_probe(config, api_key, model):
        raise RuntimeError("UnsupportedModel: model does not support agent plan")

    service._probe_video = fail_video_probe

    with pytest.raises(
        ValueError,
        match="视频模型嗅探失败（实际调用模型：doubao-seedance-2.0）",
    ):
        service._probe_model_config(
            {
                "kind": "video",
                "api_key": "test-key",
                "model": "doubao-seedance-2.0",
                "create_url": "https://provider.example/tasks",
                "query_url": "https://provider.example/tasks/{id}",
            }
        )
