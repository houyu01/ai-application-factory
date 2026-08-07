"""Regression coverage for non-video provider configuration and adapters."""

from __future__ import annotations

import base64
import json

from src.application.task_service import TaskService
from src.infrastructure.sqlite_repository import SQLiteRepository
from src.llm_service.client.dashscope_media_client import DashScopeMediaClient
from src.llm_service.client.openai_chat_client import OpenAIChatClient
from src.llm_service.client.tencent_media_client import TencentMpsAudioClient, TencentTokenHubImageClient
from src.llm_service.client.volcengine_tts_client import VolcengineTtsClient


class FakeResponse:
    """Expose one JSON provider response through urllib's context protocol."""

    def __init__(self, payload: dict) -> None:
        self.payload = payload

    def __enter__(self) -> "FakeResponse":
        return self

    def __exit__(self, *_args: object) -> bool:
        return False

    def read(self) -> bytes:
        return json.dumps(self.payload).encode()


class FakeChatCompletions:
    """Record one provider-compatible Chat Completions request."""

    def __init__(self) -> None:
        self.requests: list[dict] = []

    def create(self, **kwargs):
        self.requests.append(kwargs)
        message = type("Message", (), {"content": "OK", "tool_calls": [], "model_dump": lambda self, **_kwargs: {"role": "assistant", "content": "OK"}})()
        return type("Response", (), {"choices": [type("Choice", (), {"message": message})()]})()


class FakeChatClient:
    """Provide the subset of the OpenAI synchronous client used by the adapter."""

    def __init__(self) -> None:
        self.completions = FakeChatCompletions()
        self.chat = type("Chat", (), {"completions": self.completions})()


def test_language_chat_adapter_uses_chat_completions_and_normalizes_tools() -> None:
    """DashScope and Tencent text calls must avoid the Responses-only endpoint."""

    fake = FakeChatClient()
    client = OpenAIChatClient({"api_key": "key", "base_url": "https://example.com/v1", "model": "qwen-plus"}, sync_client=fake)
    result = client.completion(
        [{"role": "user", "content": "测试"}],
        tools=[{"type": "function", "name": "lookup", "description": "查询", "parameters": {"type": "object"}, "strict": True}],
        max_tool_rounds=0,
    )

    assert result == "OK"
    assert fake.completions.requests[0]["tools"] == [{"type": "function", "function": {"name": "lookup", "description": "查询", "parameters": {"type": "object"}}}]


def test_dashscope_image_uses_native_messages_payload() -> None:
    """DashScope image generation must not use the OpenAI Images request body."""

    requests = []

    def opener(request, data=None, timeout=None):
        assert data is None and timeout == 90
        requests.append(request)
        return FakeResponse({"output": {"choices": [{"message": {"content": [{"image": "https://cdn.example/qwen.png"}]}}]}})

    result = DashScopeMediaClient(api_key="dash-key", model="qwen-image-2.0", opener=opener).generate_image(
        "竹林小路", size="1536*2688", reference_images=["https://cdn.example/ref.png"]
    )

    assert result["url"] == "https://cdn.example/qwen.png"
    assert requests[0].get_header("Authorization") == "Bearer dash-key"
    assert json.loads(requests[0].data) == {
        "model": "qwen-image-2.0",
        "input": {"messages": [{"role": "user", "content": [{"text": "竹林小路"}, {"image": "https://cdn.example/ref.png"}]}]},
        "parameters": {"size": "1536*2688", "n": 1},
    }


def test_tencent_tokenhub_lite_and_mps_audio_use_their_own_protocols() -> None:
    """Tencent image uses an API key while MPS audio uses TC3 signing."""

    image_requests = []

    def image_opener(request, data=None, timeout=None):
        assert data is None and timeout == 90
        image_requests.append(request)
        return FakeResponse({"data": [{"url": "https://cdn.example/tencent.png"}]})

    image = TencentTokenHubImageClient(
        api_key="tokenhub-key", model="hy-image-lite", endpoint="https://tokenhub.tencentmaas.com/v1/api/image/lite", opener=image_opener
    ).generate_image("夜雨青瓦")
    assert image["url"] == "https://cdn.example/tencent.png"
    assert json.loads(image_requests[0].data) == {"model": "hy-image-lite", "prompt": "夜雨青瓦", "rsp_img_type": "url"}

    audio_requests = []

    def audio_opener(request, data=None, timeout=None):
        assert data is None and timeout == 90
        audio_requests.append(request)
        return FakeResponse({"Response": {"ErrorCode": 0, "AudioData": base64.b64encode(b"audio").decode()}})

    audio = TencentMpsAudioClient(secret_id="id", secret_key="key", voice="voice-id", opener=audio_opener).generate_audio("测试")
    assert audio["content"] == b"audio"
    assert audio_requests[0].get_header("X-tc-action") == "SyncDubbing"
    assert json.loads(audio_requests[0].data) == {"Text": "测试", "VoiceId": "voice-id"}


def test_volcengine_tts_uses_app_token_and_polls_result() -> None:
    """Volcengine audio requires its AppID/token/resource-id contract."""

    requests = []
    responses = iter([{"task_id": "tts-1", "task_status": 0}, {"task_id": "tts-1", "task_status": 1, "audio_url": "https://cdn.example/voice.mp3"}])

    def opener(request, data=None, timeout=None):
        assert data is None and timeout == 90
        requests.append(request)
        return FakeResponse(next(responses))

    result = VolcengineTtsClient(app_id="app", access_token="token", resource_id="volc.tts_async.default", voice="BV001_streaming", opener=opener).generate_audio("测试")
    assert result["url"] == "https://cdn.example/voice.mp3"
    assert requests[0].get_header("Authorization") == "Bearer; token"
    assert requests[0].get_header("Resource-id") == "volc.tts_async.default"
    submitted = json.loads(requests[0].data)
    assert {key: submitted[key] for key in submitted if key != "reqid"} == {"appid": "app", "text": "测试", "format": "mp3", "voice_type": "BV001_streaming"}
    assert len(submitted["reqid"]) == 32
    assert "appid=app&task_id=tts-1" in requests[1].full_url


def test_non_video_provider_configs_switch_fields_without_secret_leaks(tmp_path) -> None:
    """Every card persists its own provider protocol settings behind the same API."""

    service = TaskService(SQLiteRepository(tmp_path / "cloud-models.db"), object())
    service._probe_model_config = lambda _config: None
    language = service.save_model_config({"kind": "language", "provider": "dashscope", "api_key": "dash-key", "model": "qwen-plus"})
    image = service.save_model_config({"kind": "multimodal", "provider": "tencent", "api_key": "tokenhub-key", "model": "hy-image-v3.0"})
    audio = service.save_model_config({"kind": "audio", "provider": "tencent", "secret_id": "secret-id", "secret_key": "secret-key", "voice": "voice-id", "model": "mps-sync-dubbing"})

    assert language["endpoint"] == "https://dashscope.aliyuncs.com/compatible-mode/v1"
    assert image["endpoint"] == "https://tokenhub.tencentmaas.com/v1/api/image/submit"
    assert audio["endpoint"] == "https://mps.tencentcloudapi.com"
    assert audio["voice"] == "voice-id"
    assert audio["secret_key_set"] is True
    assert "secret-key" not in str(audio)


def test_project_uses_current_provider_model_when_its_saved_selection_is_stale(tmp_path) -> None:
    """A provider switch must not retain an incompatible project-level model ID."""

    service = TaskService(SQLiteRepository(tmp_path / "provider-model.db"), object())
    service._probe_model_config = lambda _config: None
    service.save_model_config({"kind": "language", "provider": "dashscope", "api_key": "dash-key", "model": "qwen-plus"})

    options = service._provider_options({"language_model": "doubao-seed"}, "language")

    assert options["provider"] == "dashscope"
    assert options["model"] == "qwen-plus"
