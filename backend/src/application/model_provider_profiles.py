"""Default provider settings shared by the model-settings service and UI adapters."""

from __future__ import annotations

from typing import Any


MODEL_PROVIDERS = {"ark", "dashscope", "tencent"}

_DEFAULTS: dict[str, dict[str, dict[str, str]]] = {
    "language": {
        "ark": {"endpoint": "https://ark.cn-beijing.volces.com/api/v3", "model": "doubao-seed-1-6-250615"},
        "dashscope": {"endpoint": "https://dashscope.aliyuncs.com/compatible-mode/v1", "model": "qwen-plus"},
        "tencent": {"endpoint": "https://api.hunyuan.cloud.tencent.com/v1", "model": "hunyuan-turbos-latest"},
    },
    "multimodal": {
        "ark": {"endpoint": "https://ark.cn-beijing.volces.com/api/plan/v3", "model": "doubao-seedream-4-0-250828"},
        "dashscope": {"endpoint": "https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation", "model": "qwen-image-2.0"},
        "tencent": {"endpoint": "https://tokenhub.tencentmaas.com/v1/api/image/submit", "model": "hy-image-v3.0"},
    },
    "audio": {
        "ark": {"create_url": "https://openspeech.bytedance.com/api/v1/tts_async/submit", "query_url": "https://openspeech.bytedance.com/api/v1/tts_async/query", "model": "volc.tts_async.default", "resource_id": "volc.tts_async.default", "voice": "BV001_streaming"},
        "dashscope": {"endpoint": "https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation", "model": "qwen3-tts-flash", "voice": "Cherry"},
        "tencent": {"endpoint": "https://mps.tencentcloudapi.com", "model": "mps-sync-dubbing", "region": "ap-guangzhou"},
    },
    "video": {
        "ark": {"create_url": "https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks", "query_url": "https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks/{id}", "model": "doubao-seedance-2.0"},
        "dashscope": {"create_url": "https://dashscope.aliyuncs.com/api/v1/services/aigc/video-generation/video-synthesis", "query_url": "https://dashscope.aliyuncs.com/api/v1/tasks/{id}", "model": "happyhorse-1.1-r2v"},
        "tencent": {"endpoint": "https://mps.tencentcloudapi.com", "create_url": "https://mps.tencentcloudapi.com", "query_url": "https://mps.tencentcloudapi.com", "model": "Hunyuan:1.5", "region": "ap-guangzhou"},
    },
}


def provider_defaults(kind: str, provider: str) -> dict[str, str]:
    """Return a copy of one supported model kind/provider default profile."""

    return dict(_DEFAULTS.get(kind, {}).get(provider, {}))


def normalized_provider(value: Any) -> str:
    """Normalize and validate a provider selection sent by the settings form."""

    provider = str(value or "ark").strip().lower()
    if provider not in MODEL_PROVIDERS:
        raise ValueError("模型服务商仅支持火山引擎、阿里云或腾讯云")
    return provider


def requires_tencent_secrets(kind: str, provider: str) -> bool:
    """Identify model protocols that authenticate through Tencent SecretId/Key."""

    return provider == "tencent" and kind in {"audio", "video"}


def requires_volc_audio_details(kind: str, provider: str) -> bool:
    """Identify the Volcengine TTS flow that needs AppID and resource metadata."""

    return kind == "audio" and provider == "ark"
