"""Tencent Cloud TokenHub image and MPS text-to-speech adapters."""

from __future__ import annotations

import base64
import json
from time import monotonic, sleep
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

from .tencent_mps_video_client import TencentMpsApiError, TencentMpsVideoClient


class TencentTokenHubImageClient:
    """Call TokenHub image endpoints configured for asset and cover generation."""

    DEFAULT_ENDPOINT = "https://tokenhub.tencentmaas.com/v1/api/image/submit"

    def __init__(self, *, api_key: str, model: str, endpoint: str = DEFAULT_ENDPOINT, opener: Any = urlopen) -> None:
        if not api_key:
            raise ValueError("腾讯云图像模型未配置 API Key")
        self.api_key = api_key
        self.model = model
        self.endpoint = endpoint.rstrip("/") or self.DEFAULT_ENDPOINT
        self._opener = opener

    def generate_image(self, prompt: str, *, reference_images: list[str] | None = None) -> dict[str, Any]:
        """Generate and, when needed, poll a TokenHub image request to completion."""

        supplied_images = [value for value in reference_images or [] if value]
        images = [value for value in supplied_images if value.startswith(("https://", "http://"))]
        if supplied_images and len(images) != len(supplied_images):
            raise ValueError("腾讯云生图的参考图必须是可公开访问的 HTTP URL；请先配置可访问的媒体存储")
        if self.endpoint.endswith("/lite"):
            if images:
                raise ValueError("腾讯云 hy-image-lite 不支持参考图；请改用 hy-image-v3.0")
            result = self._request(self.endpoint, {"model": self.model, "prompt": prompt, "rsp_img_type": "url"})
            url = self._image_url(result)
        else:
            payload: dict[str, Any] = {"model": self.model, "prompt": prompt}
            if images:
                payload["images"] = images
            created = self._request(self.endpoint, payload)
            task_id = created.get("id")
            if not isinstance(task_id, str) or not task_id:
                raise RuntimeError("腾讯云图像模型没有返回任务 ID")
            result = self._wait_for_result(task_id)
            url = self._image_url(result)
        if not url:
            raise RuntimeError("腾讯云图像模型没有返回图片 URL")
        return {"url": url, "content": None, "content_type": "image/png"}

    def _wait_for_result(self, task_id: str) -> dict[str, Any]:
        deadline = monotonic() + 180
        while monotonic() < deadline:
            result = self._request(self._query_url(), {"model": self.model, "id": task_id})
            status = str(result.get("status") or "").lower()
            if status in {"completed", "succeeded", "success"}:
                return result
            if status in {"failed", "error", "cancelled"}:
                raise RuntimeError(f"腾讯云图像任务失败：{result.get('message') or status}")
            sleep(2)
        raise RuntimeError("腾讯云图像任务超时")

    def _query_url(self) -> str:
        if self.endpoint.endswith("/submit"):
            return f"{self.endpoint[:-len('/submit')]}/query"
        return f"{self.endpoint}/query"

    def _request(self, url: str, payload: dict[str, Any]) -> dict[str, Any]:
        request = Request(
            url,
            data=json.dumps(payload, ensure_ascii=False).encode(),
            headers={"Authorization": f"Bearer {self.api_key}", "Content-Type": "application/json"},
            method="POST",
        )
        try:
            with self._opener(request, timeout=90) as response:
                result = json.loads(response.read().decode())
        except HTTPError as exc:
            detail = exc.read().decode(errors="replace")
            raise RuntimeError(f"腾讯云 TokenHub API 请求失败（HTTP {exc.code}）：{detail}") from exc
        except URLError as exc:
            raise RuntimeError(f"腾讯云 TokenHub API 网络请求失败：{exc.reason}") from exc
        if not isinstance(result, dict):
            raise RuntimeError("腾讯云 TokenHub API 返回了无效响应")
        error = result.get("error")
        if isinstance(error, dict):
            raise RuntimeError(f"腾讯云 TokenHub API 请求失败：{error.get('message') or error}")
        return result

    @staticmethod
    def _image_url(payload: dict[str, Any]) -> str | None:
        data = payload.get("data")
        if isinstance(data, dict):
            data = [data]
        if not isinstance(data, list):
            return None
        return next((item.get("url") for item in data if isinstance(item, dict) and isinstance(item.get("url"), str) and item["url"]), None)


class TencentMpsAudioClient(TencentMpsVideoClient):
    """Call Tencent MPS SyncDubbing with the same TC3 credentials as video.

    The audio settings card supplies a provider VoiceId. This client is used by
    the audio probe today and provides a reusable synchronous synthesis boundary
    for durable audio tasks when those are enabled.
    """

    def __init__(self, *, secret_id: str, secret_key: str, voice: str, region: str = "ap-guangzhou", endpoint: str = TencentMpsVideoClient.DEFAULT_ENDPOINT, opener: Any = urlopen) -> None:
        super().__init__(secret_id=secret_id, secret_key=secret_key, region=region, endpoint=endpoint, model="MPS", opener=opener)
        if not voice:
            raise ValueError("腾讯云音频模型需要配置 VoiceId")
        self.voice = voice

    def generate_audio(self, text: str) -> dict[str, Any]:
        """Synthesize text through SyncDubbing and return its audio bytes."""

        payload = self._response(self._request("SyncDubbing", {"Text": text, "VoiceId": self.voice}))
        error_code = int(payload.get("ErrorCode") or 0)
        if error_code:
            raise TencentMpsApiError(str(error_code), str(payload.get("Msg") or "SyncDubbing failed"))
        encoded = payload.get("AudioData")
        if isinstance(encoded, str) and encoded:
            return {"url": None, "content": base64.b64decode(encoded), "content_type": "audio/wav"}
        url = payload.get("AudioUrl")
        if isinstance(url, str) and url:
            return {"url": url, "content": None, "content_type": "audio/wav"}
        raise RuntimeError("腾讯云音频模型没有返回音频结果")
