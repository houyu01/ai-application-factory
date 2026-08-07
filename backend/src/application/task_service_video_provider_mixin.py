"""Provider selection helpers for durable short-drama video tasks."""

from __future__ import annotations

from typing import Any

from ..llm_service.client.ark_client import ArkClient
from ..llm_service.client.dashscope_video_client import DashScopeVideoClient
from ..llm_service.client.tencent_mps_video_client import TencentMpsVideoClient


class TaskServiceVideoProviderMixin:
    """Select Ark, DashScope, or Tencent MPS for video task lifecycle calls.

    The model settings, worker, probe, and cancellation flows share this
    boundary so each provider's asynchronous protocol remains out of routers
    and durable persistence code.
    """

    VIDEO_PROVIDERS = {"ark", "dashscope", "tencent"}

    def _video_provider_name(self, options: dict[str, Any]) -> str:
        """Return the configured provider while keeping old video settings on Ark."""

        configured = self.settings.get("video", {})
        value = options.get("provider") or configured.get("provider") or "ark"
        provider = str(value).strip().lower()
        if provider not in self.VIDEO_PROVIDERS:
            raise ValueError("视频服务商仅支持火山引擎、阿里云或腾讯云")
        return provider

    def _video_connection_options(self, options: dict[str, Any]) -> dict[str, Any]:
        """Merge stored provider credentials with project-specific model selection."""

        configured = self.settings.get("video", {})
        saved = configured if isinstance(configured, dict) else {}
        return {**saved, **options}

    def _video_task_client(self, options: dict[str, Any]) -> Any | None:
        """Build the selected provider's asynchronous video client, if applicable."""

        connection = self._video_connection_options(options)
        provider = self._video_provider_name(connection)
        model = str(connection.get("model") or "")
        if provider == "dashscope":
            return DashScopeVideoClient(
                api_key=str(connection.get("api_key") or ""),
                model=model,
                create_url=str(connection.get("create_url") or DashScopeVideoClient.DEFAULT_CREATE_URL),
                query_url=str(connection.get("query_url") or DashScopeVideoClient.DEFAULT_QUERY_URL),
            )
        if provider == "tencent":
            return TencentMpsVideoClient(
                secret_id=str(connection.get("secret_id") or ""),
                secret_key=str(connection.get("secret_key") or ""),
                region=str(connection.get("region") or "ap-guangzhou"),
                model=model or "Hunyuan:1.5",
                endpoint=str(connection.get("endpoint") or TencentMpsVideoClient.DEFAULT_ENDPOINT),
            )
        if not self._is_ark_video_provider(options):
            return None
        create_url = str(connection.get("create_url") or "")
        query_url = str(connection.get("query_url") or "")
        if not create_url or not query_url:
            raise ValueError("火山引擎视频模型必须配置创建任务 URL 和查询任务 URL")
        return ArkClient(
            api_key=str(connection.get("api_key") or ""),
            base_url=self._ark_endpoint(connection),
            model=model or "doubao-seedance-2.0",
            create_url=create_url,
            query_url=query_url,
        )

    def _video_response_reader(self, options: dict[str, Any]) -> Any:
        """Return one protocol parser without creating a new network request."""

        return {
            "ark": ArkClient,
            "dashscope": DashScopeVideoClient,
            "tencent": TencentMpsVideoClient,
        }[self._video_provider_name(options)]

    def _video_provider_supports_cancellation(self, options: dict[str, Any]) -> bool:
        """Report whether the provider exposes remote cancellation for this task API."""

        return self._video_provider_name(options) in {"ark", "dashscope"}
