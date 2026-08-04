"""Minimal Volcengine Ark media client.

Ark exposes image generation as a synchronous response and video generation as
an asynchronous task. Keeping this protocol adapter separate from the OpenAI
Responses client lets the application choose the right media API for the
configured provider without leaking provider-specific payloads into routers.
"""

from __future__ import annotations

import base64
import json
import os
import time
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import Request, urlopen


class ArkClient:
    """Call the Ark ImageGenerations and ContentsGenerationsTasks APIs."""

    def __init__(
        self,
        *,
        api_key: str,
        base_url: str = "",
        model: str,
        create_url: str | None = None,
        query_url: str | None = None,
        opener: Any = urlopen,
    ) -> None:
        if not api_key:
            raise ValueError("Ark API key is required")
        self.api_key = api_key
        self.base_url = base_url.rstrip("/")
        self.model = model
        self.create_url = (create_url or f"{self.base_url}/contents/generations/tasks").strip()
        self.query_url = (query_url or f"{self.base_url}/contents/generations/tasks/{{id}}").strip()
        self._opener = opener

    def generate_image(self, prompt: str) -> dict[str, Any]:
        """Generate an image and return either its URL or decoded bytes."""

        payload = self._request_json(
            "/images/generations",
            {
                "model": self.model,
                "prompt": prompt,
                "size": "2K",
                "sequential_image_generation": "disabled",
                "response_format": "url",
                "watermark": False,
            },
        )
        item = self._first_data_item(payload)
        url = item.get("url")
        if isinstance(url, str) and url:
            return {"url": url, "content": None, "content_type": "image/png"}
        encoded = item.get("b64_json")
        if isinstance(encoded, str) and encoded:
            return {
                "url": None,
                "content": base64.b64decode(encoded),
                "content_type": "image/png",
            }
        raise RuntimeError("Ark 图片模型没有返回 url 或 b64_json")

    def generate_video(
        self,
        prompt: str,
        *,
        ratio: str = "9:16",
        resolution: str = "720p",
        seconds: int = 8,
        reference_images: list[str] | None = None,
        poll_interval: float | None = None,
        timeout: float | None = None,
    ) -> dict[str, Any]:
        """Create an Ark video task, poll it, and return its video URL."""

        created = self.create_video_task(
            prompt,
            ratio=ratio,
            resolution=resolution,
            seconds=seconds,
            reference_images=reference_images,
        )
        task_id = created["provider_task_id"]

        interval = (
            poll_interval
            if poll_interval is not None
            else float(os.getenv("VIDEO_POLL_INTERVAL_SECONDS", "3"))
        )
        max_wait = (
            timeout
            if timeout is not None
            else float(os.getenv("VIDEO_POLL_TIMEOUT_SECONDS", "900"))
        )
        deadline = time.monotonic() + max_wait
        while True:
            result = self.get_video_task(task_id)
            status = self._read_status(result)
            if status in {"succeeded", "completed", "success", "succeed"}:
                video_url = self._read_video_url(result)
                if video_url:
                    return {
                        "url": video_url,
                        "content": None,
                        "content_type": "video/mp4",
                        "provider_id": task_id,
                    }
                raise RuntimeError("Ark 视频任务已完成，但没有返回 video_url")
            if status in {"failed", "canceled", "cancelled", "error"}:
                message = self._read_error(result) or f"任务状态：{status}"
                raise RuntimeError(f"Ark 视频生成失败：{message}")
            if time.monotonic() >= deadline:
                raise TimeoutError(f"Ark 视频任务轮询超时：{task_id}")
            time.sleep(interval)

    def create_video_task(
        self,
        prompt: str,
        *,
        ratio: str = "9:16",
        resolution: str = "720p",
        seconds: int = 8,
        reference_images: list[str] | None = None,
    ) -> dict[str, Any]:
        """Create a provider task without waiting for completion."""

        content: list[dict[str, Any]] = [{"type": "text", "text": prompt}]
        for image_url in reference_images or []:
            content.append({"type": "image_url", "image_url": {"url": image_url}})
        generation_payload: dict[str, Any] = {
            "model": self.model,
            "content": content,
            "ratio": ratio,
            "duration": seconds,
        }
        if resolution:
            generation_payload["resolution"] = resolution
        payload = self._request_json_url(self.create_url, generation_payload)
        task_id = self._read_task_id(payload)
        if not task_id:
            raise RuntimeError("Ark 视频模型没有返回任务 ID")
        return {
            "provider_task_id": task_id,
            "status": self._read_status(payload) or "queued",
            "progress": self._read_progress(payload),
            "raw": payload,
        }

    def get_video_task(self, task_id: str) -> dict[str, Any]:
        return self._request_json_url(self._query_task_url(task_id))

    def cancel_video_task(self, task_id: str) -> dict[str, Any]:
        return self._request_json_url(self._query_task_url(task_id), method="DELETE")

    def _query_task_url(self, task_id: str) -> str:
        encoded_id = quote(str(task_id), safe="")
        if "{id}" in self.query_url or "{task_id}" in self.query_url:
            return self.query_url.replace("{id}", encoded_id).replace("{task_id}", encoded_id)
        return f"{self.query_url.rstrip('/')}/{encoded_id}"

    def _request_json(
        self,
        path: str,
        payload: dict[str, Any] | None = None,
        *,
        method: str | None = None,
    ) -> dict[str, Any]:
        return self._request_json_url(
            f"{self.base_url}{path}", payload, method=method
        )

    def _request_json_url(
        self,
        url: str,
        payload: dict[str, Any] | None = None,
        *,
        method: str | None = None,
    ) -> dict[str, Any]:
        request_method = method or ("POST" if payload is not None else "GET")
        request = Request(
            url,
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {self.api_key}",
            },
            method=request_method,
            data=json.dumps(payload, ensure_ascii=False).encode() if payload is not None else None,
        )
        try:
            with self._opener(request, timeout=90) as response:
                return json.loads(response.read().decode())
        except HTTPError as exc:
            detail = exc.read().decode(errors="replace")
            raise RuntimeError(f"Ark API 请求失败（HTTP {exc.code}）：{detail}") from exc
        except URLError as exc:
            raise RuntimeError(f"Ark API 网络请求失败：{exc.reason}") from exc

    @staticmethod
    def _first_data_item(payload: dict[str, Any]) -> dict[str, Any]:
        data = payload.get("data")
        if isinstance(data, list) and data and isinstance(data[0], dict):
            return data[0]
        raise RuntimeError("Ark 图片模型返回了空数据")

    @staticmethod
    def _read_status(payload: dict[str, Any]) -> str:
        data = payload.get("data")
        nested = data[0] if isinstance(data, list) and data and isinstance(data[0], dict) else data
        if not isinstance(nested, dict):
            nested = {}
        status = payload.get("status") or payload.get("task_status") or nested.get("status") or nested.get("task_status")
        if isinstance(status, str):
            return status.lower()
        return ""

    @staticmethod
    def _read_progress(payload: dict[str, Any]) -> int:
        data = payload.get("data")
        nested = data[0] if isinstance(data, list) and data and isinstance(data[0], dict) else data
        if not isinstance(nested, dict):
            nested = {}
        value = payload.get("progress", nested.get("progress", 0))
        try:
            return max(0, min(100, int(float(value))))
        except (TypeError, ValueError):
            return 0

    @staticmethod
    def _read_task_id(payload: dict[str, Any]) -> str | None:
        candidates: list[Any] = [payload.get("id"), payload.get("task_id")]
        data = payload.get("data")
        if isinstance(data, dict):
            candidates.extend([data.get("id"), data.get("task_id")])
        elif isinstance(data, list) and data and isinstance(data[0], dict):
            candidates.extend([data[0].get("id"), data[0].get("task_id")])
        for candidate in candidates:
            if isinstance(candidate, str) and candidate.strip():
                return candidate.strip()
        return None

    @classmethod
    def _read_video_url(cls, payload: dict[str, Any]) -> str | None:
        candidates: list[Any] = [payload.get("video_url"), payload.get("url")]
        content = payload.get("content")
        if isinstance(content, dict):
            candidates.extend([content.get("video_url"), content.get("url")])
        elif isinstance(content, list):
            candidates.extend(
                item.get("video_url") or item.get("url")
                for item in content
                if isinstance(item, dict)
            )
        data = payload.get("data")
        if isinstance(data, list) and data and isinstance(data[0], dict):
            candidates.extend([data[0].get("video_url"), data[0].get("url")])
        for candidate in candidates:
            if isinstance(candidate, dict):
                candidate = candidate.get("url")
            if isinstance(candidate, str) and candidate:
                return candidate
        return None

    @staticmethod
    def _read_error(payload: dict[str, Any]) -> str | None:
        error = payload.get("error")
        if isinstance(error, dict):
            message = error.get("message") or error.get("detail")
            if isinstance(message, str):
                return message
        for key in ("message", "error_message"):
            message = payload.get(key)
            if isinstance(message, str) and message:
                return message
        return None
