"""Provider-specific image and audio dispatch for TaskService workflows."""

from __future__ import annotations

from typing import Any

from ..llm_service.client.ark_client import ArkClient
from ..llm_service.client.dashscope_media_client import DashScopeMediaClient
from ..llm_service.client.openai_client import OpenAICLient, OpenAIClientBaseOptions
from ..llm_service.client.tencent_media_client import TencentMpsAudioClient, TencentTokenHubImageClient
from ..llm_service.client.volcengine_tts_client import VolcengineTtsClient
from .model_provider_profiles import normalized_provider


AI_GENERATED_IMAGE_TAG_PROMPT = (
    "标识要求（必须遵守，优先级最高）：在画面左上角添加“AI生成”标签。"
    "标签使用小型圆角矩形、深色半透明底、细浅色描边与浅灰文字，"
    "距上边和左边保留安全边距，不遮挡主体；除该标签外不添加其他文字或水印。"
)


class TaskServiceMediaProviderMixin:
    """Route configured image and audio requests to Ark, DashScope, or Tencent.

    Asset/cover workers and settings probes call this boundary. It owns provider
    choice and credentials while the surrounding workflows only receive a
    normalized URL-or-bytes result that can be persisted through MediaStore.
    """

    def _model_provider_name(self, options: dict[str, Any]) -> str:
        """Return a validated provider for a non-video model configuration."""

        return normalized_provider(options.get("provider"))

    @staticmethod
    def _image_prompt_with_ai_generated_tag(prompt: str) -> str:
        """Append the mandatory provenance label to every provider image prompt."""

        return "\n\n".join(part for part in (prompt.strip(), AI_GENERATED_IMAGE_TAG_PROMPT) if part)

    def _generate_provider_image(self, options: dict[str, Any], prompt: str, *, ratio: str = "9:16", reference_images: list[str] | None = None) -> dict[str, Any]:
        """Generate one image through the selected cloud provider."""

        prompt = self._image_prompt_with_ai_generated_tag(prompt)
        provider = self._model_provider_name(options)
        api_key = str(options.get("api_key") or "")
        model = str(options.get("model") or "")
        if provider == "ark":
            if not options.get("provider") and not self._is_ark_image_provider(options):
                size = "1024x1536" if ratio == "9:16" else "1536x1024"
                return OpenAICLient(
                    OpenAIClientBaseOptions(api_key=api_key, base_url=options.get("endpoint"), model=model or "gpt-image-1")
                ).generate_image(prompt, model=model or "gpt-image-1", size=size, n=1)
            return ArkClient(api_key=api_key, base_url=self._ark_endpoint(options), model=model or "doubao-seedream-4-0-250828").generate_image(
                prompt, reference_images=reference_images, size="2K"
            )
        if provider == "dashscope":
            size = "1536*2688" if ratio == "9:16" else "2688*1536"
            return DashScopeMediaClient(api_key=api_key, model=model or "qwen-image-2.0", endpoint=str(options.get("endpoint") or "")).generate_image(
                prompt, size=size, reference_images=reference_images
            )
        return TencentTokenHubImageClient(api_key=api_key, model=model or "hy-image-v3.0", endpoint=str(options.get("endpoint") or "")).generate_image(
            prompt, reference_images=reference_images
        )

    def _generate_provider_audio(self, options: dict[str, Any], text: str) -> dict[str, Any]:
        """Synthesize text through the selected cloud provider's native API."""

        provider = self._model_provider_name(options)
        voice = str(options.get("voice") or "")
        if provider == "ark":
            return VolcengineTtsClient(
                app_id=str(options.get("app_id") or ""),
                access_token=str(options.get("api_key") or ""),
                resource_id=str(options.get("resource_id") or ""),
                voice=voice,
                create_url=str(options.get("create_url") or ""),
                query_url=str(options.get("query_url") or ""),
            ).generate_audio(text)
        if provider == "dashscope":
            return DashScopeMediaClient(
                api_key=str(options.get("api_key") or ""),
                model=str(options.get("model") or "qwen3-tts-flash"),
                endpoint=str(options.get("endpoint") or ""),
            ).generate_audio(text, voice=voice or "Cherry")
        return TencentMpsAudioClient(
            secret_id=str(options.get("secret_id") or ""),
            secret_key=str(options.get("secret_key") or ""),
            voice=voice,
            region=str(options.get("region") or "ap-guangzhou"),
            endpoint=str(options.get("endpoint") or TencentMpsAudioClient.DEFAULT_ENDPOINT),
        ).generate_audio(text)
