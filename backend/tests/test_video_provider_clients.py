"""Protocol and settings regression coverage for selectable video providers."""

from __future__ import annotations

import json

import pytest

from src.application.task_service import TaskService
from src.domain.models import ProjectCreate
from src.infrastructure.sqlite_repository import SQLiteRepository
from src.llm_service.client.dashscope_video_client import DashScopeVideoClient
from src.llm_service.client.tencent_mps_video_client import TencentMpsVideoClient


class FakeResponse:
    """Expose a JSON HTTP response to the lightweight provider adapters."""

    def __init__(self, payload: dict) -> None:
        self.payload = payload

    def __enter__(self) -> "FakeResponse":
        return self

    def __exit__(self, *_args: object) -> bool:
        return False

    def read(self) -> bytes:
        return json.dumps(self.payload).encode()


def test_dashscope_client_rejects_happyhorse_first_frame_models() -> None:
    """DashScope image-to-video models cannot silently use native first frames."""

    client = DashScopeVideoClient(
        api_key="dash-key", model="happyhorse-1.1-i2v"
    )

    with pytest.raises(ValueError, match="不使用该模式"):
        client.create_video_task(
            "一只猫在草地上奔跑",
            reference_images=["https://cdn.example/first.png"],
        )


def test_dashscope_client_sends_all_happyhorse_reference_images() -> None:
    """HappyHorse R2V preserves the project's reference order and image mentions."""

    requests = []

    def opener(request, data=None, timeout=None):
        requests.append(request)
        assert data is None
        assert timeout == 90
        return FakeResponse({"output": {"task_id": "dash-task", "task_status": "PENDING"}})

    client = DashScopeVideoClient(
        api_key="dash-key", model="happyhorse-1.1-r2v", opener=opener
    )
    client.create_video_task(
        "@图1 中的角色挥动 @图2 中的折扇。",
        ratio="16:9",
        resolution="720p",
        seconds=5,
        reference_images=[
            "https://cdn.example/character.png",
            "https://cdn.example/fan.png",
            "https://cdn.example/character.png",
        ],
    )

    assert json.loads(requests[0].data) == {
        "model": "happyhorse-1.1-r2v",
        "input": {
            "prompt": "[Image 1] 中的角色挥动 [Image 2] 中的折扇。",
            "media": [
                {"type": "reference_image", "url": "https://cdn.example/character.png"},
                {"type": "reference_image", "url": "https://cdn.example/fan.png"},
            ],
        },
        "parameters": {"resolution": "720P", "ratio": "16:9", "duration": 5},
    }
    assert "first_frame" not in requests[0].data.decode()


def test_dashscope_client_sends_wan_r2v_reference_images() -> None:
    """The configured Wan R2V snapshot uses its reference-image protocol."""

    requests = []

    def opener(request, data=None, timeout=None):
        assert data is None
        assert timeout == 90
        requests.append(request)
        return FakeResponse({"output": {"task_id": "dash-task", "task_status": "PENDING"}})

    client = DashScopeVideoClient(
        api_key="dash-key", model="wan2.7-r2v-2026-06-12", opener=opener
    )
    client.create_video_task(
        "@图1 中的角色走进 @图2 中的庭院。",
        ratio="9:16",
        resolution="720p",
        seconds=5,
        reference_images=[
            "https://cdn.example/character.png",
            "https://cdn.example/courtyard.png",
        ],
    )

    assert json.loads(requests[0].data) == {
        "model": "wan2.7-r2v-2026-06-12",
        "input": {
            "prompt": "图1 中的角色走进 图2 中的庭院。",
            "media": [
                {"type": "reference_image", "url": "https://cdn.example/character.png"},
                {"type": "reference_image", "url": "https://cdn.example/courtyard.png"},
            ],
        },
        "parameters": {"resolution": "720P", "ratio": "9:16", "duration": 5},
    }


def test_dashscope_client_parses_task_result_and_requires_happyhorse_references() -> None:
    """A DashScope provider result becomes a successful durable video URL."""

    reference_client = DashScopeVideoClient(
        api_key="dash-key", model="happyhorse-1.1-r2v"
    )
    with pytest.raises(ValueError, match="1 到 9 张参考图"):
        reference_client.create_video_task("测试视频")

    result = {
        "output": {
            "task_id": "dash-task",
            "task_status": "SUCCEEDED",
            "results": {"video_url": "https://cdn.example/dashscope.mp4"},
        }
    }
    assert reference_client._read_status(result) == "succeeded"
    assert reference_client._read_progress(result) == 100
    assert reference_client._read_video_url(result) == "https://cdn.example/dashscope.mp4"
    assert reference_client._read_video_url({
        "output": {"video_url": "https://cdn.example/dashscope-direct.mp4"}
    }) == "https://cdn.example/dashscope-direct.mp4"


def test_tencent_mps_client_signs_create_and_parses_polled_video() -> None:
    """Tencent MPS uses TC3 authentication instead of a bearer API key."""

    responses = iter(
        [
            {"Response": {"TaskId": "mps-task", "RequestId": "request-1"}},
            {
                "Response": {
                    "Status": "DONE",
                    "VideoUrls": ["https://cdn.example/tencent.mp4"],
                }
            },
        ]
    )
    requests = []

    def opener(request, data=None, timeout=None):
        assert data is None
        assert timeout == 90
        requests.append(request)
        return FakeResponse(next(responses))

    client = TencentMpsVideoClient(
        secret_id="secret-id",
        secret_key="secret-key",
        region="ap-guangzhou",
        model="Vidu:q3",
        opener=opener,
    )
    created = client.create_video_task(
        "一只猫在草地上奔跑",
        ratio="9:16",
        resolution="720p",
        seconds=5,
        reference_images=[
            "https://cdn.example/character.png",
            "https://cdn.example/scene.png",
            "https://cdn.example/character.png",
        ],
    )
    result = client.get_video_task(created["provider_task_id"])

    assert created["provider_task_id"] == "mps-task"
    assert requests[0].full_url == TencentMpsVideoClient.DEFAULT_ENDPOINT
    assert requests[0].get_header("Authorization").startswith("TC3-HMAC-SHA256 Credential=secret-id/")
    assert requests[0].get_header("X-tc-action") == "CreateAigcVideoTask"
    assert json.loads(requests[0].data) == {
        "ModelName": "Vidu",
        "ModelVersion": "q3",
        "Prompt": "一只猫在草地上奔跑",
        "Duration": 5,
        "ExtraParameters": {"Resolution": "720P", "AspectRatio": "9:16"},
        "ImageInfos": [
            {"ImageUrl": "https://cdn.example/character.png"},
            {"ImageUrl": "https://cdn.example/scene.png"},
        ],
    }
    assert TencentMpsVideoClient._read_status(result) == "succeeded"
    assert TencentMpsVideoClient._read_video_url(result) == "https://cdn.example/tencent.mp4"


def test_tencent_client_rejects_native_first_frame_fallback() -> None:
    """Tencent models without multi-image references must not receive ImageUrl."""

    client = TencentMpsVideoClient(
        secret_id="secret-id", secret_key="secret-key", model="Hunyuan:1.5"
    )

    with pytest.raises(ValueError, match="不使用该模式"):
        client.create_video_task(
            "一只猫在草地上奔跑",
            reference_images=["https://cdn.example/first.png"],
        )


def test_video_provider_config_selects_dashscope_or_tencent_without_secret_leaks(tmp_path) -> None:
    """The settings page stores provider choice and masks Tencent credentials."""

    service = TaskService(SQLiteRepository(tmp_path / "provider-settings.db"), object())
    service._probe_model_config = lambda _config: None
    dashscope = service.save_model_config(
        {"kind": "video", "provider": "dashscope", "api_key": "dash-key"}
    )
    tencent = service.save_model_config(
        {
            "kind": "video",
            "provider": "tencent",
            "secret_id": "secret-id",
            "secret_key": "secret-key",
            "region": "ap-guangzhou",
            "model": "Hunyuan:1.5",
        }
    )

    assert dashscope["provider"] == "dashscope"
    assert dashscope["model"] == "happyhorse-1.1-r2v"
    assert dashscope["create_url"] == DashScopeVideoClient.DEFAULT_CREATE_URL
    assert tencent["provider"] == "tencent"
    assert tencent["create_url"] == TencentMpsVideoClient.DEFAULT_ENDPOINT
    assert tencent["region"] == "ap-guangzhou"
    assert tencent["secret_key_set"] is True
    assert "secret-key" not in str(tencent)
    assert isinstance(service._video_task_client({"model": "Hunyuan:1.5"}), TencentMpsVideoClient)


@pytest.mark.parametrize(
    "model",
    ["happyhorse-1.1-r2v", "wan2.7-r2v-2026-06-12"],
)
def test_dashscope_r2v_probe_uses_required_reference_and_supported_resolution(
    tmp_path, monkeypatch, model
) -> None:
    """Each DashScope R2V configuration probes with its mandatory reference image."""

    service = TaskService(SQLiteRepository(tmp_path / "dashscope-probe.db"), object())
    submitted: dict[str, object] = {}

    class ProbeClient:
        def create_video_task(self, _prompt, **kwargs):
            submitted.update(kwargs)
            return {"provider_task_id": "probe-task"}

        def get_video_task(self, _task_id):
            return {"output": {"task_status": "PENDING"}}

        def cancel_video_task(self, _task_id):
            return {}

    monkeypatch.setattr(service, "_video_task_client", lambda _config: ProbeClient())
    service._probe_video(
        {"provider": "dashscope", "model": model},
        "dash-key",
        model,
    )

    assert submitted["resolution"] == "720p"
    assert submitted["reference_images"] == ["https://cdn.translate.alibaba.com/r/wanx-demo-1.png"]


def test_worker_refreshes_video_provider_after_another_process_saves_settings(tmp_path) -> None:
    """A stale worker instance must not submit a newly selected model to Ark."""

    database = tmp_path / "shared-settings.db"
    worker = TaskService(SQLiteRepository(database), object())
    api = TaskService(SQLiteRepository(database), object())
    api._probe_model_config = lambda _config: None
    api.save_model_config({
        "kind": "video", "provider": "dashscope", "api_key": "dash-key",
        "model": "happyhorse-1.1-r2v",
    })
    project = worker.create_project(ProjectCreate(
        name="阿里云短剧", script="小林在黄昏的车站捡到一张泛黄的车票。",
        video_model="happyhorse-1.1-r2v",
    ))

    options = worker._provider_options(worker.get_project(project["id"]), "video")

    assert isinstance(worker._video_task_client(options), DashScopeVideoClient)
