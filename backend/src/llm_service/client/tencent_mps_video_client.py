"""Tencent Cloud MPS asynchronous AIGC video protocol adapter."""

from __future__ import annotations

from datetime import datetime, timezone
import hashlib
import hmac
import json
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlparse
from urllib.request import Request, urlopen


class TencentMpsApiError(RuntimeError):
    """Carry Tencent's provider error code for side-effect-free credential probes."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(f"腾讯云 MPS API 请求失败（{code}）：{message}")
        self.code = code


class TencentMpsVideoClient:
    """Call Tencent MPS AIGC video APIs with TC3-HMAC-SHA256 authentication.

    The selected Tencent video provider uses this client from durable task
    workers. It owns request signing, reference-image payload conversion, task
    polling, and immediate persistence of the short-lived result URL.
    """

    DEFAULT_ENDPOINT = "https://mps.tencentcloudapi.com"
    VERSION = "2019-06-12"
    REFERENCE_IMAGE_MODELS = {
        "vidu": {"q2", "q2-pro", "q3-turbo", "q3", "q3-mix"},
        "kling": {"1.6", "o1", "3.0-omni"},
        "pixverse": {"v5.6", "v6", "c1"},
        "h2": {"1.0"},
    }

    def __init__(
        self,
        *,
        secret_id: str,
        secret_key: str,
        region: str = "ap-guangzhou",
        model: str = "Hunyuan:1.5",
        endpoint: str = DEFAULT_ENDPOINT,
        opener: Any = urlopen,
    ) -> None:
        if not secret_id or not secret_key:
            raise ValueError("腾讯云视频模型需要配置 SecretId 和 SecretKey")
        self.secret_id = secret_id
        self.secret_key = secret_key
        self.region = region or "ap-guangzhou"
        self.model = model or "Hunyuan:1.5"
        self.endpoint = endpoint.rstrip("/") or self.DEFAULT_ENDPOINT
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
        """Submit a Tencent MPS task with its multi-image reference protocol."""

        model_name, model_version = self._model_parts()
        payload: dict[str, Any] = {
            "ModelName": model_name,
            "Prompt": prompt[:2000],
            "Duration": int(seconds),
            "ExtraParameters": {
                "Resolution": str(resolution).upper(),
                "AspectRatio": ratio,
            },
        }
        if model_version:
            payload["ModelVersion"] = model_version
        images = list(dict.fromkeys(str(image) for image in reference_images or [] if image))
        if images:
            if not self._supports_reference_images(model_name, model_version):
                raise ValueError(
                    f"腾讯云模型“{self.model}”只支持接口原生首帧图，"
                    "本应用不使用该模式。请改用支持多图参考的 Vidu、Kling、PixVerse 或 H2 模型。"
                )
            payload["ImageInfos"] = [{"ImageUrl": image} for image in images]
        response = self._request("CreateAigcVideoTask", payload)
        task_id = self._response(response).get("TaskId")
        if not isinstance(task_id, str) or not task_id:
            raise RuntimeError("腾讯云视频模型没有返回任务 ID")
        return {"provider_task_id": task_id, "status": "queued", "progress": 5, "raw": response}

    def get_video_task(self, task_id: str) -> dict[str, Any]:
        """Poll one Tencent MPS task for state, progress, and the video URL."""

        return self._request("DescribeAigcVideoTask", {"TaskId": task_id})

    def cancel_video_task(self, task_id: str) -> dict[str, Any]:
        """Declare that Tencent MPS exposes no cancel action for this task type."""

        return {"status": "unsupported", "task_id": task_id}

    def probe_credentials(self) -> None:
        """Validate credentials/signing without creating a potentially billable video."""

        try:
            self.get_video_task("probe-invalid-task")
        except TencentMpsApiError as exc:
            if exc.code.startswith(("InvalidParameter", "ResourceNotFound", "FailedOperation")):
                return
            raise

    def _model_parts(self) -> tuple[str, str]:
        for separator in (":", "/"):
            if separator in self.model:
                name, version = self.model.split(separator, 1)
                return name.strip() or "Hunyuan", version.strip()
        return self.model.strip() or "Hunyuan", ""

    @classmethod
    def _supports_reference_images(cls, model_name: str, model_version: str) -> bool:
        """Return whether Tencent documents multi-image references for this model."""

        return model_version.lower() in cls.REFERENCE_IMAGE_MODELS.get(
            model_name.lower(), set()
        )

    def _request(self, action: str, payload: dict[str, Any]) -> dict[str, Any]:
        encoded = json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode()
        now = datetime.now(timezone.utc)
        timestamp = int(now.timestamp())
        host = urlparse(self.endpoint).netloc
        headers = self._headers(host, action, encoded, now.strftime("%Y-%m-%d"), timestamp)
        request = Request(self.endpoint, data=encoded, headers=headers, method="POST")
        try:
            with self._opener(request, timeout=90) as response:
                result = json.loads(response.read().decode())
        except HTTPError as exc:
            detail = exc.read().decode(errors="replace")
            raise RuntimeError(f"腾讯云 MPS API 请求失败（HTTP {exc.code}）：{detail}") from exc
        except URLError as exc:
            raise RuntimeError(f"腾讯云 MPS API 网络请求失败：{exc.reason}") from exc
        error = self._response(result).get("Error")
        if isinstance(error, dict):
            raise TencentMpsApiError(str(error.get("Code") or "Unknown"), str(error.get("Message") or ""))
        return result

    def _headers(
        self, host: str, action: str, payload: bytes, date: str, timestamp: int
    ) -> dict[str, str]:
        canonical_headers = (
            "content-type:application/json\n"
            f"host:{host}\n"
            f"x-tc-action:{action.lower()}\n"
        )
        signed_headers = "content-type;host;x-tc-action"
        canonical_request = "\n".join(
            ("POST", "/", "", canonical_headers, signed_headers, self._sha256(payload))
        )
        scope = f"{date}/mps/tc3_request"
        string_to_sign = "\n".join(
            ("TC3-HMAC-SHA256", str(timestamp), scope, self._sha256(canonical_request.encode()))
        )
        secret_date = self._hmac(f"TC3{self.secret_key}".encode(), date)
        secret_service = self._hmac(secret_date, "mps")
        secret_signing = self._hmac(secret_service, "tc3_request")
        signature = self._hmac(secret_signing, string_to_sign)
        authorization = (
            f"TC3-HMAC-SHA256 Credential={self.secret_id}/{scope}, "
            f"SignedHeaders={signed_headers}, Signature={signature.hex()}"
        )
        return {
            "Authorization": authorization,
            "Content-Type": "application/json",
            "Host": host,
            "X-TC-Action": action,
            "X-TC-Region": self.region,
            "X-TC-Timestamp": str(timestamp),
            "X-TC-Version": self.VERSION,
        }

    @staticmethod
    def _sha256(value: bytes) -> str:
        return hashlib.sha256(value).hexdigest()

    @staticmethod
    def _hmac(key: bytes, value: str) -> bytes:
        return hmac.new(key, value.encode(), hashlib.sha256).digest()

    @staticmethod
    def _response(payload: dict[str, Any]) -> dict[str, Any]:
        response = payload.get("Response")
        return response if isinstance(response, dict) else {}

    @classmethod
    def _read_status(cls, payload: dict[str, Any]) -> str:
        status = cls._response(payload).get("Status")
        return {"wait": "queued", "run": "running", "done": "succeeded", "fail": "failed"}.get(
            str(status).lower(), str(status).lower()
        )

    @classmethod
    def _read_progress(cls, payload: dict[str, Any]) -> int:
        return {"queued": 5, "running": 50, "succeeded": 100}.get(cls._read_status(payload), 0)

    @classmethod
    def _read_video_url(cls, payload: dict[str, Any]) -> str | None:
        urls = cls._response(payload).get("VideoUrls")
        if isinstance(urls, list):
            return next((item for item in urls if isinstance(item, str) and item), None)
        return None

    @classmethod
    def _read_error(cls, payload: dict[str, Any]) -> str | None:
        message = cls._response(payload).get("Message")
        return message if isinstance(message, str) and message else None
