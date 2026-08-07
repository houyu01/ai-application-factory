"""Volcengine asynchronous text-to-speech protocol adapter."""

from __future__ import annotations

import json
from time import monotonic, sleep
from typing import Any
from uuid import uuid4
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen


class VolcengineTtsClient:
    """Submit and poll the Volcengine speech TTS API chosen on the audio card.

    The audio settings probe uses this adapter to confirm AppID, Access Token,
    resource ID, and voice type together. The adapter keeps the vendor-specific
    bearer-token convention out of application services.
    """

    DEFAULT_CREATE_URL = "https://openspeech.bytedance.com/api/v1/tts_async/submit"
    DEFAULT_QUERY_URL = "https://openspeech.bytedance.com/api/v1/tts_async/query"

    def __init__(self, *, app_id: str, access_token: str, resource_id: str, voice: str, create_url: str = DEFAULT_CREATE_URL, query_url: str = DEFAULT_QUERY_URL, opener: Any = urlopen) -> None:
        if not app_id or not access_token or not resource_id or not voice:
            raise ValueError("火山引擎音频模型需要配置 AppID、Access Token、Resource-Id 和 Voice Type")
        self.app_id = app_id
        self.access_token = access_token
        self.resource_id = resource_id
        self.voice = voice
        self.create_url = create_url.rstrip("/") or self.DEFAULT_CREATE_URL
        self.query_url = query_url.rstrip("/") or self.DEFAULT_QUERY_URL
        self._opener = opener

    def generate_audio(self, text: str) -> dict[str, Any]:
        """Create and poll a small or long-text TTS task until its URL is ready."""

        created = self._request(
            self.create_url,
            {"appid": self.app_id, "reqid": uuid4().hex, "text": text, "format": "mp3", "voice_type": self.voice},
        )
        task_id = created.get("task_id")
        if not isinstance(task_id, str) or not task_id:
            raise RuntimeError(f"火山引擎音频模型没有返回任务 ID：{created.get('message') or ''}")
        deadline = monotonic() + 90
        while monotonic() < deadline:
            result = self._request(f"{self.query_url}?{urlencode({'appid': self.app_id, 'task_id': task_id})}")
            status = str(result.get("task_status") or "")
            if status == "1":
                url = result.get("audio_url")
                if isinstance(url, str) and url:
                    return {"url": url, "content": None, "content_type": "audio/mpeg"}
                raise RuntimeError("火山引擎音频任务成功但没有返回 audio_url")
            if status == "2":
                raise RuntimeError(f"火山引擎音频任务失败：{result.get('message') or '未知错误'}")
            sleep(1)
        raise RuntimeError("火山引擎音频任务超时")

    def _request(self, url: str, payload: dict[str, Any] | None = None) -> dict[str, Any]:
        request = Request(
            url,
            data=json.dumps(payload, ensure_ascii=False).encode() if payload is not None else None,
            headers={"Authorization": f"Bearer; {self.access_token}", "Resource-Id": self.resource_id, "Content-Type": "application/json"},
            method="POST" if payload is not None else "GET",
        )
        try:
            with self._opener(request, timeout=90) as response:
                result = json.loads(response.read().decode())
        except HTTPError as exc:
            detail = exc.read().decode(errors="replace")
            raise RuntimeError(f"火山引擎音频 API 请求失败（HTTP {exc.code}）：{detail}") from exc
        except URLError as exc:
            raise RuntimeError(f"火山引擎音频 API 网络请求失败：{exc.reason}") from exc
        if not isinstance(result, dict):
            raise RuntimeError("火山引擎音频 API 返回了无效响应")
        if result.get("code") not in (None, 0):
            raise RuntimeError(f"火山引擎音频 API 请求失败：{result.get('code')}：{result.get('message') or ''}")
        return result
