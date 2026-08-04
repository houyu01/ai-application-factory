"""Provider probes used before model settings are persisted."""

from __future__ import annotations

import logging
import os
from typing import Any

from ..llm_service.client.ark_client import ArkClient
from ..llm_service.client.openai_client import OpenAICLient, OpenAIClientBaseOptions

logger = logging.getLogger(__name__)


class TaskServiceModelProbeMixin:
    """Verify configured model credentials and endpoints before saving them.

    The settings page calls this indirectly through ``save_model_config``. The
    probe uses the smallest real provider request available for each model kind
    so a saved configuration is immediately usable by generation tasks.
    """

    def _probe_model_config(self, config: dict[str, Any]) -> None:
        kind = str(config.get("kind") or "")
        api_key = str(config.get("api_key") or os.getenv("OPENAI_API_KEY") or "")
        model = str(config.get("model") or "").strip()
        if not api_key:
            raise ValueError(f"{self._model_kind_label(kind)}模型未配置 API Key")
        if not model:
            raise ValueError(f"{self._model_kind_label(kind)}模型未配置模型名称")
        try:
            if kind == "language":
                self._probe_openai_text(config, api_key, model)
            elif kind == "multimodal":
                self._probe_image(config, api_key, model)
            elif kind == "video":
                self._probe_video(config, api_key, model)
            elif kind == "audio":
                self._probe_audio(config, api_key, model)
            else:
                raise ValueError(f"不支持嗅探的模型类型：{kind}")
        except ValueError:
            raise
        except Exception as exc:
            detail = str(exc).strip() or exc.__class__.__name__
            raise ValueError(f"{self._model_kind_label(kind)}模型嗅探失败：{detail}") from exc

    def _openai_client(self, config: dict[str, Any], api_key: str, model: str) -> OpenAICLient:
        return OpenAICLient(
            OpenAIClientBaseOptions(
                api_key=api_key,
                base_url=str(config.get("endpoint") or "").strip() or None,
                model=model,
            )
        )

    def _probe_openai_text(self, config: dict[str, Any], api_key: str, model: str) -> None:
        result = self._openai_client(config, api_key, model).completion(
            [{"role": "user", "content": "请只回复：OK"}], max_tool_rounds=0
        )
        if not result.strip():
            raise RuntimeError("模型没有返回文本结果")

    def _probe_image(self, config: dict[str, Any], api_key: str, model: str) -> None:
        options = {"endpoint": config.get("endpoint"), "model": model}
        if self._is_ark_image_provider(options):
            result = ArkClient(
                api_key=api_key,
                base_url=self._ark_endpoint(options),
                model=model,
            ).generate_image("生成一张简单的纯色测试图")
        else:
            result = self._openai_client(config, api_key, model).generate_image(
                "生成一张简单的纯色测试图", model=model, size="1024x1024", n=1
            )
        if not result.get("url") and not result.get("content"):
            raise RuntimeError("图片模型没有返回有效结果")

    def _probe_video(self, config: dict[str, Any], api_key: str, model: str) -> None:
        create_url = str(config.get("create_url") or "").strip()
        query_url = str(config.get("query_url") or "").strip()
        if not create_url or not query_url:
            raise ValueError("视频模型必须配置创建任务 URL 和查询任务 URL")
        client = ArkClient(
            api_key=api_key,
            base_url=self._ark_endpoint(config),
            model=model,
            create_url=create_url,
            query_url=query_url,
        )
        created = client.create_video_task(
            "生成一个简单的测试视频：静态风景，镜头缓慢推进。",
            ratio="16:9",
            resolution="480p",
            seconds=4,
        )
        task_id = str(created.get("provider_task_id") or "")
        if not task_id:
            raise RuntimeError("视频模型没有返回任务 ID")
        try:
            queried = client.get_video_task(task_id)
            if not isinstance(queried, dict):
                raise RuntimeError("查询视频任务没有返回有效结果")
        finally:
            try:
                client.cancel_video_task(task_id)
            except Exception as exc:  # Cleanup failure must not hide a valid probe.
                logger.warning("视频模型嗅探任务取消失败：%s", exc)

    def _probe_audio(self, config: dict[str, Any], api_key: str, model: str) -> None:
        self._openai_client(config, api_key, model).probe_audio("模型连接测试")

    @staticmethod
    def _model_kind_label(kind: str) -> str:
        return {"language": "语言", "multimodal": "图像", "video": "视频", "audio": "音频"}.get(kind, kind)
