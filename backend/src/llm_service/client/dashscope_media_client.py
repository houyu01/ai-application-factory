"""DashScope image and text-to-speech protocol adapter."""

from __future__ import annotations

import base64
import json
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


class DashScopeMediaClient:
    """Call DashScope native image and TTS APIs selected in the settings page.

    Asset-image workers and audio configuration probes use this adapter. It
    owns the native DashScope payloads because neither API follows the OpenAI
    Images or Speech endpoint shape.
    """

    DEFAULT_ENDPOINT = "https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation"

    def __init__(self, *, api_key: str, model: str, endpoint: str = DEFAULT_ENDPOINT, opener: Any = urlopen) -> None:
        if not api_key:
            raise ValueError("阿里云模型未配置 API Key")
        self.api_key = api_key
        self.model = model
        self.endpoint = endpoint.rstrip("/") or self.DEFAULT_ENDPOINT
        self._opener = opener

    def generate_image(self, prompt: str, *, size: str, reference_images: list[str] | None = None) -> dict[str, Any]:
        """Generate one DashScope image and return its short-lived provider URL."""

        content: list[dict[str, str]] = [{"text": prompt}]
        content.extend({"image": value} for value in reference_images or [] if value)
        payload = {
            "model": self.model,
            "input": {"messages": [{"role": "user", "content": content}]},
            "parameters": {"size": size.replace("x", "*"), "n": 1},
        }
        result = self._request(payload)
        url = self._image_url(result)
        if not url:
            raise RuntimeError("阿里云图片模型没有返回图片 URL")
        return {"url": url, "content": None, "content_type": "image/png"}

    def generate_audio(self, text: str, *, voice: str) -> dict[str, Any]:
        """Synthesize short text and return DashScope's downloadable audio result."""

        result = self._request({"model": self.model, "input": {"text": text, "voice": voice, "language_type": "Chinese"}})
        output = self._output(result)
        audio = output.get("audio") if isinstance(output.get("audio"), dict) else {}
        url = audio.get("url")
        if isinstance(url, str) and url:
            return {"url": url, "content": None, "content_type": "audio/wav"}
        encoded = audio.get("data")
        if isinstance(encoded, str) and encoded:
            return {"url": None, "content": base64.b64decode(encoded), "content_type": "audio/wav"}
        raise RuntimeError("阿里云音频模型没有返回音频结果")

    def _request(self, payload: dict[str, Any]) -> dict[str, Any]:
        request = Request(
            self.endpoint,
            data=json.dumps(payload, ensure_ascii=False).encode(),
            headers={"Authorization": f"Bearer {self.api_key}", "Content-Type": "application/json"},
            method="POST",
        )
        try:
            with self._opener(request, timeout=90) as response:
                data = response.read().decode()
        except HTTPError as exc:
            detail = exc.read().decode(errors="replace")
            raise RuntimeError(f"阿里云 DashScope API 请求失败（HTTP {exc.code}）：{detail}") from exc
        except URLError as exc:
            raise RuntimeError(f"阿里云 DashScope API 网络请求失败：{exc.reason}") from exc
        result = self._json_or_last_sse_event(data)
        if not isinstance(result, dict):
            raise RuntimeError("阿里云 DashScope API 返回了无效响应")
        if result.get("code") and result.get("code") not in {"200", 200}:
            raise RuntimeError(f"阿里云 DashScope API 请求失败：{result.get('code')}：{result.get('message') or ''}")
        return result

    @staticmethod
    def _json_or_last_sse_event(value: str) -> dict[str, Any]:
        try:
            payload = json.loads(value)
            return payload if isinstance(payload, dict) else {}
        except json.JSONDecodeError:
            events = [line[5:].strip() for line in value.splitlines() if line.startswith("data:")]
            for event in reversed(events):
                if event and event != "[DONE]":
                    try:
                        payload = json.loads(event)
                    except json.JSONDecodeError:
                        continue
                    if isinstance(payload, dict):
                        return payload
        return {}

    @staticmethod
    def _output(payload: dict[str, Any]) -> dict[str, Any]:
        output = payload.get("output")
        return output if isinstance(output, dict) else {}

    @classmethod
    def _image_url(cls, payload: dict[str, Any]) -> str | None:
        output = cls._output(payload)
        candidates: list[Any] = []
        results = output.get("results")
        if isinstance(results, dict):
            results = [results]
        if isinstance(results, list):
            candidates.extend(item.get("url") for item in results if isinstance(item, dict))
        choices = output.get("choices")
        if isinstance(choices, list):
            for choice in choices:
                message = choice.get("message") if isinstance(choice, dict) else None
                content = message.get("content") if isinstance(message, dict) else None
                if isinstance(content, list):
                    candidates.extend(item.get("image") or item.get("url") for item in content if isinstance(item, dict))
        return next((str(value) for value in candidates if isinstance(value, str) and value), None)
