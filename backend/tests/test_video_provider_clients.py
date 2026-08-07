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


def test_dashscope_client_uses_async_header_and_happyhorse_first_frame() -> None:
    """DashScope must never call HappyHorse synchronously or with Ark payloads."""

    requests = []
    calls = []

    def opener(request, data=None, timeout=None):
        requests.append(request)
        calls.append((data, timeout))
        return FakeResponse({"output": {"task_id": "dash-task", "task_status": "PENDING"}})

    client = DashScopeVideoClient(
        api_key="dash-key", model="happyhorse-1.1-i2v", opener=opener
    )
    task = client.create_video_task(
        "一只猫在草地上奔跑",
        resolution="720p",
        seconds=5,
        reference_images=["https://cdn.example/first.png", "https://cdn.example/ignored.png"],
    )

    assert task["provider_task_id"] == "dash-task"
    assert calls == [(None, 90)]
    assert requests[0].full_url == DashScopeVideoClient.DEFAULT_CREATE_URL
    assert requests[0].get_header("X-dashscope-async") == "enable"
    assert json.loads(requests[0].data) == {
        "model": "happyhorse-1.1-i2v",
        "input": {
            "prompt": "一只猫在草地上奔跑",
            "media": [{"type": "first_frame", "url": "https://cdn.example/first.png"}],
        },
        "parameters": {"resolution": "720P", "duration": 5},
    }


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


def test_dashscope_client_parses_task_result_and_requires_happyhorse_frame() -> None:
    """A DashScope provider result becomes a successful durable video URL."""

    client = DashScopeVideoClient(api_key="dash-key", model="happyhorse-1.1-i2v")
    with pytest.raises(ValueError, match="首帧参考图"):
        client.create_video_task("测试视频")

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
    assert client._read_status(result) == "succeeded"
    assert client._read_progress(result) == 100
    assert client._read_video_url(result) == "https://cdn.example/dashscope.mp4"
    assert client._read_video_url({
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
        model="Hunyuan:1.5",
        opener=opener,
    )
    created = client.create_video_task(
        "一只猫在草地上奔跑",
        ratio="9:16",
        resolution="720p",
        seconds=5,
        reference_images=["https://cdn.example/first.png"],
    )
    result = client.get_video_task(created["provider_task_id"])

    assert created["provider_task_id"] == "mps-task"
    assert requests[0].full_url == TencentMpsVideoClient.DEFAULT_ENDPOINT
    assert requests[0].get_header("Authorization").startswith("TC3-HMAC-SHA256 Credential=secret-id/")
    assert requests[0].get_header("X-tc-action") == "CreateAigcVideoTask"
    assert json.loads(requests[0].data) == {
        "ModelName": "Hunyuan",
        "ModelVersion": "1.5",
        "Prompt": "一只猫在草地上奔跑",
        "Duration": 5,
        "ExtraParameters": {"Resolution": "720P", "AspectRatio": "9:16"},
        "ImageUrl": "https://cdn.example/first.png",
    }
    assert TencentMpsVideoClient._read_status(result) == "succeeded"
    assert TencentMpsVideoClient._read_video_url(result) == "https://cdn.example/tencent.mp4"


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


def test_dashscope_probe_uses_the_required_reference_and_supported_resolution(tmp_path, monkeypatch) -> None:
    """Saving a HappyHorse R2V configuration probes through its asynchronous contract."""

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
        {"provider": "dashscope", "model": "happyhorse-1.1-r2v"},
        "dash-key",
        "happyhorse-1.1-r2v",
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
