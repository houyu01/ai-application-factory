from datetime import datetime, timedelta, timezone
from io import BytesIO
import json
import logging
import os
import re
from urllib.request import Request, urlopen
from urllib.parse import urlparse
from typing import Any

from PIL import Image, ImageDraw, ImageOps

from ..domain.models import GenerationStatus
from ..infrastructure.sqlite_repository import SQLiteRepository
from ..infrastructure.media_store import media_store
from ..llm_service.planner import ScriptPlanner
from ..llm_service.client.ark_client import ArkClient
from ..llm_service.client.openai_client import OpenAICLient, OpenAIClientBaseOptions

logger = logging.getLogger(__name__)
def utc_now_after(seconds: int) -> str:
    return (datetime.now(timezone.utc) + timedelta(seconds=seconds)).isoformat()

class TaskServiceProviderMixin:
    """Behavior slice of TaskService."""

    def _provider_options(self, project: dict[str, Any], kind: str) -> dict[str, Any]:
        configured: dict[str, Any] = {}
        if kind == "video":
            # Prefer a dedicated video endpoint, while preserving the existing
            # shared multimodal endpoint as a backwards-compatible fallback.
            configured.update(self.settings.get("multimodal", {}))
            configured.update(self.settings.get("video", {}))
        elif kind == "multimodal":
            configured.update(self.settings.get("multimodal", {}))
        else:
            configured.update(self.settings.get(kind, {}))
        if kind == "language":
            model = project.get("language_model") or configured.get("model")
        elif kind == "video":
            model = project.get("video_model") or configured.get("model")
        else:
            model = project.get("multimodal_model") or configured.get("model")
        return {
            "api_key": configured.get("api_key") or os.getenv("OPENAI_API_KEY"),
            "endpoint": configured.get("endpoint") or os.getenv("OPENAI_BASE_URL"),
            "create_url": configured.get("create_url"), "query_url": configured.get("query_url"),
            "model": model,
            "style": project.get("style"),
            "theme": project.get("theme"),
            "ratio": project.get("ratio"),
            "resolution": project.get("resolution", "720p"),
            "shot_constraints": project.get("shot_constraints") or {},
        }
    def _public_model_config(self, kind: str) -> dict[str, Any]:
        configured = self.settings.get(kind, {})
        if kind == "video" and not configured:
            # Older installations stored video_model alongside the shared
            # multimodal credentials. Preserve that model in the video card.
            shared = self.settings.get("multimodal", {})
            legacy_video_model = shared.get("video_model") if isinstance(shared, dict) else None
            configured = dict(shared) if isinstance(shared, dict) else {}
            if legacy_video_model:
                configured["model"] = legacy_video_model
                configured["models"] = [legacy_video_model]
            else:
                configured.pop("model", None)
                configured.pop("models", None)
        if not isinstance(configured, dict):
            configured = {}
        models = configured.get("models")
        if not isinstance(models, list):
            models = []
        normalized_models: list[str] = []
        for value in models:
            name = str(value).strip()
            if name and name not in normalized_models:
                normalized_models.append(name)
        default_model = str(configured.get("model") or "").strip()
        if not normalized_models:
            normalized_models = list(self.MODEL_DEFAULTS.get(kind, []))
        if not default_model and normalized_models:
            default_model = normalized_models[0]
        api_key = str(configured.get("api_key") or os.getenv("OPENAI_API_KEY") or "")
        return {
            "kind": kind,
            "endpoint": str(configured.get("endpoint") or ""),
            "model": default_model,
            "models": normalized_models,
            "api_key_set": bool(api_key),
            "api_key_masked": self._mask_secret(api_key),
            "create_url": str(configured.get("create_url") or ""), "query_url": str(configured.get("query_url") or ""),
        }
    def save_storage_config(self, config: dict[str, Any]) -> dict[str, Any]:
        """Validate, persist, and activate the selected media backend."""

        previous = self.settings.get("storage", {})
        normalized = {
            "provider": str(config.get("provider") or "local").strip().lower(),
            "endpoint": str(config.get("endpoint") or "").strip(),
            "bucket": str(config.get("bucket") or "").strip(),
            "region": str(config.get("region") or "").strip(),
            "secret_id": str(config.get("secret_id") or "").strip(),
            "secret_key": str(config.get("secret_key") or "").strip(),
            "prefix": str(config.get("prefix") or "media").strip(),
            "public_base_url": str(config.get("public_base_url") or "").strip(),
        }
        if normalized["provider"] != "local":
            if not normalized["secret_id"]:
                normalized["secret_id"] = str(previous.get("secret_id") or "")
            if not normalized["secret_key"]:
                normalized["secret_key"] = str(previous.get("secret_key") or "")
        else:
            # Do not retain cloud credentials when the active backend is local.
            normalized["secret_id"] = ""
            normalized["secret_key"] = ""

        # Probe on an isolated candidate so generation workers keep using the
        # current backend until upload and public download both succeed.
        normalized = media_store.probe_config(normalized)
        media_store.configure(normalized)
        self.settings["storage"] = normalized
        self.repository.set_setting("storage", normalized)
        return media_store.public_config()
    def _persist_provider_result(
        self, result: dict[str, Any], extension: str, provider_label: str
    ) -> str:
        media_store.configure(self.repository.get_setting("storage", {}) or {})
        url = result.get("url")
        if isinstance(url, str) and url:
            return self._persist_media_url(url, extension)
        content = result.get("content")
        if isinstance(content, bytes):
            return media_store.save(content, extension)
        raise RuntimeError(f"{provider_label}没有返回可保存的结果")
    @staticmethod
    def _video_reference_images(
        project: dict[str, Any], shot: dict[str, Any], public_media_base_url: str | None = None
    ) -> list[str]:
        assets_by_id = {
            str(asset.get("id")): asset
            for asset in project.get("assets", [])
            if asset.get("id")
        }
        references: list[str] = []
        seen: set[str] = set()
        nodes = shot.get("prompt_rich") or []
        if not isinstance(nodes, list):
            return references
        for node in nodes:
            if not isinstance(node, dict) or node.get("type") != "reference":
                continue
            image_url = media_store.provider_reference_url(node.get("image_url") or assets_by_id.get(str(node.get("asset_id") or ""), {}).get("image_url"), public_media_base_url)
            if not isinstance(image_url, str) or not image_url or image_url in seen:
                continue
            seen.add(image_url)
            references.append(image_url)
        return references
    @staticmethod
    def _persist_media_url(url: str, extension: str) -> str:
        """Copy a provider URL into the active store before exposing it."""

        if url.startswith("/api/media/"):
            return url
        return media_store.save_url(url, extension)
    def get_model_configs(self) -> dict[str, dict[str, Any]]:
        """Return model choices without returning API keys to the browser."""

        return {kind: self._public_model_config(kind) for kind in self.MODEL_DEFAULTS}
    @staticmethod
    def _mask_secret(value: Any) -> str:
        secret = str(value or "")
        if not secret:
            return ""
        return "*" * max(8, min(16, len(secret)))
    @staticmethod
    def _is_ark_image_provider(options: dict[str, Any]) -> bool:
        endpoint = str(options.get("create_url") or options.get("endpoint") or "").lower()
        model = str(options.get("model") or "").lower()
        if options.get("create_url") or options.get("query_url"):
            return True
        return (
            ("ark." in endpoint or (not endpoint and "doubao" in model))
            and "seed" in model
            and "dream" in model
        )
    @staticmethod
    def _asset_generation_prompt(project: dict[str, Any], asset: dict[str, Any]) -> str:
        asset_type = str(asset.get("type") or "prop")
        configured_prompts = project.get("asset_public_prompts") or {}
        public_prompt = ""
        if isinstance(configured_prompts, dict):
            public_prompt = str(configured_prompts.get(asset_type) or "").strip()
        if not public_prompt:
            public_prompt = {
                "character": (
                    f"图片风格为「{project.get('style') or '真人风格'}」，"
                    "生成全身正视图以及一张面部特写（左边占二分之一的位置是超级大的"
                    "正面面部特写，右边是二分之一放一张从头到鞋子的正视图，纯白背景，纯白背景）。"
                ),
                "scene": (
                    f"图片风格为「{project.get('style') or '真人风格'}」，"
                    "保持空间结构清晰、主体建筑或环境可识别，画面完整，"
                    "适合作为短剧场景素材参考图。"
                ),
                "prop": (
                    f"图片风格为「{project.get('style') or '真人风格'}」，"
                    "主体道具清晰完整，材质、纹理和关键特征明确，画面干净，"
                    "适合作为短剧道具素材参考图。"
                ),
            }.get(asset_type, "生成清晰、可复用的素材设定图。")
        asset_prompt = str(asset.get("prompt") or "").strip()
        return "\n\n".join(part for part in (public_prompt, asset_prompt) if part)
    @staticmethod
    def _ark_endpoint(options: dict[str, Any]) -> str:
        endpoint = str(options.get("endpoint") or "").strip().rstrip("/")
        if endpoint and not options.get("create_url"):
            # Accept both the Ark API base URL and a copied operation URL from
            # the documentation. The client appends the operation path below.
            for suffix in ("/images/generations", "/contents/generations/tasks"):
                if endpoint.endswith(suffix):
                    endpoint = endpoint[: -len(suffix)].rstrip("/")
                    break
            return endpoint
        return "https://ark.cn-beijing.volces.com/api/plan/v3"
    def _generate_image_url(self, project: dict[str, Any], asset: dict[str, Any]) -> str:
        options = self._provider_options(project, "multimodal")
        if not options.get("api_key"):
            raise RuntimeError("未配置多模态模型 API Key，无法生成素材图片")
        prompt = self._asset_generation_prompt(project, asset)
        logger.info(
            "Starting image generation: project=%s asset=%s type=%s model=%s endpoint=%s",
            project.get("id"),
            asset.get("id"),
            asset.get("type"),
            options.get("model") or "未配置",
            options.get("endpoint") or "默认",
        )
        if self._is_ark_image_provider(options):
            result = ArkClient(
                api_key=options["api_key"],
                base_url=self._ark_endpoint(options),
                model=options.get("model") or "doubao-seedream-4-0-250828",
            ).generate_image(prompt)
            return self._persist_provider_result(result, ".png", "Ark 图片模型")
        client = OpenAICLient(
            OpenAIClientBaseOptions(
                api_key=options["api_key"],
                base_url=options.get("endpoint"),
                model=options.get("model") or "gpt-image-1",
            )
        )
        image_size = "1024x1536" if project.get("ratio") == "9:16" else "1536x1024"
        result = client.generate_image(
            prompt,
            model=options.get("model") or "gpt-image-1",
            size=image_size,
            n=1,
        )
        return self._persist_provider_result(result, ".png", "图片模型")
    def _assets_with_voice_details(self, assets: list[dict[str, Any]]) -> list[dict[str, Any]]:
        """Expose the selected voice description to prompt-generation skills."""

        enriched: list[dict[str, Any]] = []
        for asset in assets:
            voice_id = str(asset.get("voice_id") or "").strip()
            voice = self.repository.get_voice_preset(voice_id) if voice_id else None
            if voice is None:
                enriched.append(dict(asset))
                continue
            enriched.append(
                {
                    **asset,
                    "voice_name": voice["name"],
                    "voice_prompt": voice["prompt"],
                }
            )
        return enriched
    def get_storage_config(self) -> dict[str, Any]:
        return media_store.public_config()
    def _video_generation_prompt(self, project: dict[str, Any], shot: dict[str, Any]) -> str:
        public_prompt = str(project.get("video_public_prompt") or "").strip()
        if not public_prompt:
            public_prompt = (
                f"整体保持{project.get('style') or '真人风格'}，"
                f"题材为{project.get('theme') or '都市'}，按剧本处理方式组织镜头。"
            )
        shot_prompt = str(shot.get("prompt") or "").strip()
        constraints = project.get("shot_constraints") or {}
        constraint_prompt = (
            f"分镜约束：{'需要字幕' if constraints.get('subtitles') else '不要字幕'}；"
            f"{'需要背景音乐' if constraints.get('background_music') else '不要背景音乐'}。"
        )
        referenced_ids = {
            str(node.get("asset_id"))
            for node in (shot.get("prompt_rich") or [])
            if isinstance(node, dict)
            and node.get("type") == "reference"
            and node.get("asset_id")
        }
        voice_lines: list[str] = []
        for asset in project.get("assets", []):
            if asset.get("type") != "character" or not asset.get("voice_id"):
                continue
            if referenced_ids and str(asset.get("id")) not in referenced_ids:
                continue
            voice = self.repository.get_voice_preset(asset.get("voice_id"))
            if voice:
                voice_lines.append(
                    f"角色音色：{asset.get('name', '角色')}使用{voice['name']}；音色提示词：{voice['prompt']}"
                )
        voice_prompt = "\n".join(voice_lines)
        return "\n\n".join(
            part for part in (public_prompt, constraint_prompt, voice_prompt, shot_prompt) if part
        )
    @staticmethod
    def _is_ark_video_provider(options: dict[str, Any]) -> bool:
        create_url = str(options.get("create_url") or "").lower()
        query_url = str(options.get("query_url") or "").lower()
        if create_url or query_url:
            return True
        endpoint = str(options.get("endpoint") or "").lower()
        model = str(options.get("model") or "").lower()
        return (
            ("ark." in endpoint or (not endpoint and "doubao" in model))
            and "seed" in model
            and "dance" in model
        )
    def _generate_video_url(
        self, project: dict[str, Any], shot: dict[str, Any], public_media_base_url: str | None = None
    ) -> str | None:
        """Generate a shot video through Ark, a custom endpoint, or OpenAI."""

        options = self._provider_options(project, "video")
        prompt = self._video_generation_prompt(project, shot)
        reference_images = self._video_reference_images(project, shot, public_media_base_url)
        endpoint = os.getenv("VIDEO_GENERATION_ENDPOINT")
        if endpoint and not options.get("create_url"):
            request = Request(
                endpoint,
                data=json.dumps(
                    {
                        "model": options.get("model"),
                        "prompt": prompt,
                        "ratio": project.get("ratio", "9:16"),
                        "resolution": options.get("resolution", "720p"),
                        "duration": int(shot.get("duration_seconds") or shot.get("duration") or 10),
                        "reference_images": reference_images,
                    }
                ).encode(),
                headers={
                    "Content-Type": "application/json",
                    **(
                        {"Authorization": f"Bearer {options['api_key']}"}
                        if options.get("api_key")
                        else {}
                    ),
                },
                method="POST",
            )
            with urlopen(request, timeout=90) as response:  # noqa: S310 - configured endpoint
                payload = json.loads(response.read().decode())
            if isinstance(payload, dict):
                result = payload.get("video_url") or payload.get("url")
                if isinstance(result, str) and result:
                    return self._persist_media_url(result, ".mp4")
                data = payload.get("data")
                if isinstance(data, list) and data and isinstance(data[0], dict):
                    result = data[0].get("video_url") or data[0].get("url")
                    if isinstance(result, str) and result:
                        return self._persist_media_url(result, ".mp4")
            raise RuntimeError("视频模型没有返回 video_url/url")

        if not options.get("api_key"):
            raise RuntimeError("未配置多模态模型 API Key，无法生成视频")
        if self._is_ark_video_provider(options):
            if not options.get("create_url") or not options.get("query_url"):
                raise RuntimeError("视频模型必须配置创建任务 URL 和查询任务 URL")
            result = ArkClient(
                api_key=options["api_key"],
                base_url=self._ark_endpoint(options),
                model=options.get("model") or "doubao-seedance-2.0",
                create_url=options["create_url"],
                query_url=options["query_url"],
            ).generate_video(
                prompt,
                ratio=project.get("ratio", "9:16"),
                resolution=options.get("resolution", "720p"),
                seconds=8,
                reference_images=reference_images,
            )
            return self._persist_provider_result(result, ".mp4", "Ark 视频模型")
        client = OpenAICLient(
            OpenAIClientBaseOptions(
                api_key=options["api_key"],
                base_url=options.get("endpoint"),
                model=options.get("model") or "sora-2",
            )
        )
        result = client.generate_video(
            prompt,
            model=options.get("model") or "sora-2",
            ratio=project.get("ratio", "9:16"),
            resolution=options.get("resolution", "720p"),
            seconds=8,
            reference_images=reference_images,
        )
        return self._persist_provider_result(result, ".mp4", "视频模型")

    def save_model_config(self, config: dict[str, Any]) -> dict[str, Any]:
        kind = str(config["kind"])
        previous = self.settings.get(kind, {})
        normalized = dict(config)
        if not normalized.get("api_key") and previous.get("api_key"):
            normalized["api_key"] = previous["api_key"]
        for key in ("create_url", "query_url"):
            if not normalized.get(key) and previous.get(key): normalized[key] = previous[key]
        if kind == "video" and (not normalized.get("create_url") or not normalized.get("query_url")):
            raise ValueError("视频模型必须配置创建任务 URL 和查询任务 URL")
        model = str(normalized.get("model") or normalized.get("video_model") or "").strip()
        raw_models = normalized.get("models")
        if not isinstance(raw_models, list):
            raw_models = previous.get("models", [])
        models: list[str] = []
        for value in raw_models:
            name = str(value).strip()
            if name and name not in models:
                models.append(name)
        if model and model not in models:
            models.insert(0, model)
        if not models:
            models = list(self.MODEL_DEFAULTS.get(kind, []))
        if not model and models:
            model = models[0]
        normalized["model"] = model
        normalized["models"] = models
        normalized.pop("video_model", None)
        self._probe_model_config(normalized)
        self.settings[kind] = normalized
        self.repository.set_setting(kind, normalized)
        if kind == "language" and isinstance(self.planner, ScriptPlanner):
            self.planner.configure(
                {
                    "api_key": normalized.get("api_key"),
                    "endpoint": normalized.get("endpoint"),
                    "model": normalized.get("model"),
                }
            )
        return {"status": "saved", "kind": kind, **self._public_model_config(kind)}
