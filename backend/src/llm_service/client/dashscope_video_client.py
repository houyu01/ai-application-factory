"""DashScope asynchronous video-generation protocol adapter."""

from __future__ import annotations

import json
import re
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import Request, urlopen


class DashScopeVideoClient:
    """Call DashScope's asynchronous video API for a durable shot-video task.

    ``TaskServiceVideoProviderMixin`` selects this adapter when the video
    settings page is set to Alibaba Cloud. It owns DashScope's header, payload,
    task-result parsing, and best-effort queued-task cancellation boundary.
    """

    DEFAULT_CREATE_URL = (
        "https://dashscope.aliyuncs.com/api/v1/services/aigc/video-generation/video-synthesis"
    )
    DEFAULT_QUERY_URL = "https://dashscope.aliyuncs.com/api/v1/tasks/{id}"

    def __init__(
        self,
        *,
        api_key: str,
        model: str,
        create_url: str = DEFAULT_CREATE_URL,
        query_url: str = DEFAULT_QUERY_URL,
        opener: Any = urlopen,
    ) -> None:
        if not api_key:
            raise ValueError("阿里云视频模型未配置 API Key")
        self.api_key = api_key
        self.model = model
        self.create_url = create_url.strip() or self.DEFAULT_CREATE_URL
        self.query_url = query_url.strip() or self.DEFAULT_QUERY_URL
        self._opener = opener

    def create_video_task(
        self,
        prompt: str,
        *,
        ratio: str = "9:16",
        resolution: str = "720p",
        seconds: int = 8,
        reference_images: list[str] | None = None,
    ) -> dict[str, Any]:
        """Submit a DashScope task using prompt-directed reference images only."""

        images = list(dict.fromkeys(str(image) for image in reference_images or [] if image))
        reference_to_video = self._uses_reference_images()
        if self._is_first_frame_model():
            raise ValueError(
                f"阿里云模型“{self.model}”仅支持接口原生首帧图模式，"
                "本应用不使用该模式。请改用 happyhorse-1.1-r2v。"
            )
        reference_limit = self._reference_image_limit()
        if reference_to_video and not 1 <= len(images) <= reference_limit:
            raise ValueError(
                f"阿里云模型“{self.model}”需要传入 1 到 {reference_limit} 张参考图。"
            )
        if images and not reference_to_video:
            raise ValueError(
                f"阿里云模型“{self.model}”不支持参考图生视频。"
                "请改用支持 reference_image 的 R2V 模型。"
            )
        media = self._media(images)
        input_data: dict[str, Any] = {
            "prompt": self._reference_prompt(prompt) if reference_to_video else prompt
        }
        if media:
            input_data["media"] = media
        parameters: dict[str, Any] = {
            "resolution": str(resolution).upper(),
            "duration": int(seconds),
        }
        if reference_to_video:
            parameters["ratio"] = ratio
        payload = {
            "model": self.model,
            "input": input_data,
            "parameters": parameters,
        }
        result = self._request(self.create_url, payload)
        task_id = self._read_task_id(result)
        if not task_id:
            raise RuntimeError("阿里云视频模型没有返回任务 ID")
        return {
            "provider_task_id": task_id,
            "status": self._read_status(result) or "queued",
            "progress": self._read_progress(result),
            "raw": result,
        }

    def get_video_task(self, task_id: str) -> dict[str, Any]:
        """Retrieve the current DashScope task result by its provider task id."""

        return self._request(self._task_url(task_id))

    def cancel_video_task(self, task_id: str) -> dict[str, Any]:
        """Cancel a queued DashScope task without changing the local audit row."""

        return self._request(f"{self._task_url(task_id).rstrip('/')}/cancel", method="POST")

    def _is_first_frame_model(self) -> bool:
        """Identify image-to-video models that require a native first frame."""

        return self.model.lower().endswith("-i2v")

    def _uses_reference_images(self) -> bool:
        """Return whether this DashScope model accepts ordered reference images."""

        model = self.model.lower()
        return (
            "happyhorse" in model and "-r2v" in model
        ) or model.startswith("wan2.7-r2v")

    def _reference_image_limit(self) -> int:
        """Return the documented R2V image cap for the selected DashScope model."""

        return 5 if self.model.lower().startswith("wan2.7-r2v") else 9

    @staticmethod
    def _media(images: list[str]) -> list[dict[str, str]]:
        """Map ordered project references without falling back to a first frame."""

        return [{"type": "reference_image", "url": image} for image in images]

    def _reference_prompt(self, prompt: str) -> str:
        """Translate editor reference markers to the selected R2V prompt syntax."""

        if self.model.lower().startswith("wan2.7-r2v"):
            return re.sub(r"@图\s*(\d+)", r"图\1", prompt)
        return re.sub(r"@图\s*(\d+)", r"[Image \1]", prompt)

    def _task_url(self, task_id: str) -> str:
        encoded = quote(str(task_id), safe="")
        if "{id}" in self.query_url or "{task_id}" in self.query_url:
            return self.query_url.replace("{id}", encoded).replace("{task_id}", encoded)
        return f"{self.query_url.rstrip('/')}/{encoded}"

    def _request(
        self,
        url: str,
        payload: dict[str, Any] | None = None,
        *,
        method: str | None = None,
    ) -> dict[str, Any]:
        request = Request(
            url,
            data=json.dumps(payload, ensure_ascii=False).encode() if payload is not None else None,
            headers={
                "Authorization": f"Bearer {self.api_key}",
                "Content-Type": "application/json",
                "X-DashScope-Async": "enable",
            },
            method=method or ("POST" if payload is not None else "GET"),
        )
        try:
            with self._opener(request, timeout=90) as response:
                return json.loads(response.read().decode())
        except HTTPError as exc:
            detail = exc.read().decode(errors="replace")
            raise RuntimeError(f"阿里云 DashScope API 请求失败（HTTP {exc.code}）：{detail}") from exc
        except URLError as exc:
            raise RuntimeError(f"阿里云 DashScope API 网络请求失败：{exc.reason}") from exc

    @staticmethod
    def _output(payload: dict[str, Any]) -> dict[str, Any]:
        output = payload.get("output")
        return output if isinstance(output, dict) else {}

    @classmethod
    def _read_task_id(cls, payload: dict[str, Any]) -> str | None:
        task_id = cls._output(payload).get("task_id")
        return task_id.strip() if isinstance(task_id, str) and task_id.strip() else None

    @classmethod
    def _read_status(cls, payload: dict[str, Any]) -> str:
        status = cls._output(payload).get("task_status")
        return status.lower() if isinstance(status, str) else ""

    @classmethod
    def _read_progress(cls, payload: dict[str, Any]) -> int:
        return {"pending": 5, "running": 50, "succeeded": 100}.get(
            cls._read_status(payload), 0
        )

    @classmethod
    def _read_video_url(cls, payload: dict[str, Any]) -> str | None:
        output = cls._output(payload)
        direct_url = output.get("video_url") or output.get("url")
        if isinstance(direct_url, str) and direct_url:
            return direct_url
        results = output.get("results")
        if isinstance(results, dict):
            results = [results]
        if not isinstance(results, list):
            return None
        for item in results:
            if not isinstance(item, dict):
                continue
            value = item.get("video_url") or item.get("url")
            if isinstance(value, str) and value:
                return value
        return None

    @classmethod
    def _read_error(cls, payload: dict[str, Any]) -> str | None:
        output = cls._output(payload)
        code = output.get("code") or payload.get("code")
        message = output.get("message") or payload.get("message")
        detail = "：".join(str(item) for item in (code, message) if item)
        return detail or None
