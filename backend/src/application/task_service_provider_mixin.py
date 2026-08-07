from datetime import datetime, timedelta, timezone
from io import BytesIO
import base64
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
VIDEO_REFERENCE_IMAGE_REVIEW_NOTICE = (
    "生成视频中所有的参考图，均为seedream生成的图片，并不是真人，请认真审核查看"
)
def utc_now_after(seconds: int) -> str:
    return (datetime.now(timezone.utc) + timedelta(seconds=seconds)).isoformat()
class TaskServiceProviderMixin:
    """Behavior slice of TaskService."""
    def _provider_options(self, project: dict[str, Any], kind: str) -> dict[str, Any]:
        current = self._refresh_setting(kind)
        configured: dict[str, Any] = {}
        if kind == "video":
            # Prefer a dedicated video endpoint, while preserving the existing
            # shared multimodal endpoint as a backwards-compatible fallback.
            configured.update(self._refresh_setting("multimodal"))
            configured.update(current)
        elif kind == "multimodal":
            configured.update(current)
        else:
            configured.update(current)
        field = {"language": "language_model", "multimodal": "multimodal_model", "video": "video_model"}.get(kind)
        selected_model = str(project.get(field) or "") if field else ""
        configured_models = [str(value) for value in configured.get("models", []) if str(value)]
        model = selected_model if selected_model and (not configured_models or selected_model in configured_models) else configured.get("model")
        return {
            "api_key": configured.get("api_key") or os.getenv("OPENAI_API_KEY"),
            "endpoint": configured.get("endpoint") or os.getenv("OPENAI_BASE_URL"),
            "create_url": configured.get("create_url"), "query_url": configured.get("query_url"), "provider": configured.get("provider"), "region": configured.get("region"), "secret_id": configured.get("secret_id"), "secret_key": configured.get("secret_key"), "app_id": configured.get("app_id"), "resource_id": configured.get("resource_id"), "voice": configured.get("voice"),
            "model": model,
            "style": project.get("style"),
            "theme": project.get("theme"),
            "ratio": project.get("ratio"),
            "resolution": project.get("resolution", "720p"),
            "shot_constraints": project.get("shot_constraints") or {},
            "episode_count": project.get("episode_count", 50),
            "expanded_script_min_chars": project.get("expanded_script_min_chars", 50_000),
            "expanded_script_max_chars": project.get("expanded_script_max_chars", 100_000),
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
            referenced = assets_by_id.get(str(node.get("asset_id") or ""), {})
            if node.get("asset_type") == "placeholder" and (referenced.get("metadata") or {}).get("render_mode") != "generated_composite":
                continue
            image_url = media_store.provider_reference_url(node.get("image_url") or referenced.get("image_url"), public_media_base_url)
            if not isinstance(image_url, str) or not image_url or image_url in seen:
                continue
            seen.add(image_url)
            references.append(image_url)
        return references

    @staticmethod
    def _video_boundary_frames(
        shot: dict[str, Any], public_media_base_url: str | None = None
    ) -> dict[str, str]:
        """Resolve saved first/last frame images into provider-readable URLs."""

        frames = shot.get("first_last_frames") or {}
        resolved: dict[str, str] = {}
        for side in ("first", "last"):
            value = frames.get(side) if isinstance(frames, dict) else None
            raw_url = value.get("url") if isinstance(value, dict) else value
            if isinstance(raw_url, str) and raw_url.startswith("data:image/"):
                try:
                    header, encoded = raw_url.split(",", 1)
                    extension = ".png" if "png" in header else ".jpg"
                    media_url = media_store.save(base64.b64decode(encoded), extension, "image/png" if extension == ".png" else "image/jpeg")
                    raw_url = media_store.provider_reference_url(media_url, public_media_base_url)
                except (ValueError, base64.binascii.Error):
                    raw_url = None
            else:
                raw_url = media_store.provider_reference_url(raw_url, public_media_base_url)
            if isinstance(raw_url, str) and raw_url:
                resolved[side] = raw_url
        return resolved

    def _video_generation_inputs(
        self,
        project: dict[str, Any],
        shot: dict[str, Any],
        public_media_base_url: str | None = None,
    ) -> tuple[str, list[str]]:
        """Build ordered references and prompt-directed frame controls.

        The video worker calls this before creating any provider task. Provider
        clients map the common ``@图N`` notation to their native media protocol;
        saved boundary frames are normal references and the prompt identifies
        their exact image numbers.
        """

        reference_images = self._video_reference_images(
            project, shot, public_media_base_url
        )
        boundary_frames = self._video_boundary_frames(
            shot, public_media_base_url
        )
        frame_instructions: list[str] = []
        for side in ("first", "last"):
            image_url = boundary_frames.get(side)
            if not image_url:
                continue
            reference_index = len(reference_images) + 1
            reference_images.append(image_url)
            if side == "first":
                frame_instructions.append(
                    f"@图{reference_index} 是视频首帧：视频第一帧必须以该图的主体、构图、光线和状态开始。"
                )
            else:
                frame_instructions.append(
                    f"@图{reference_index} 是视频尾帧：视频最后一帧必须收束到该图的主体、构图、光线和状态。"
                )
        prompt = self._video_generation_prompt(project, shot)
        if not frame_instructions:
            return prompt, reference_images
        frame_prompt = (
            "首尾帧控制（最高优先级）：输入参考图与 @图编号按相同顺序对应。\n"
            + "\n".join(frame_instructions)
        )
        return "\n\n".join((prompt, frame_prompt)), reference_images

    @staticmethod
    def _persist_media_url(url: str, extension: str) -> str:
        """Copy a provider URL into the active store before exposing it."""

        if url.startswith("/api/media/"):
            return url
        return media_store.save_url(url, extension)
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
        style_prompt = f"整体图片生成风格采用「{project.get('style') or '真人风格'}」。"
        configured_prompts = project.get("asset_public_prompts") or {}
        public_prompt = ""
        if isinstance(configured_prompts, dict):
            public_prompt = str(configured_prompts.get(asset_type) or "").strip()
        if not public_prompt:
            public_prompt = {
                "character": (
                    "生成完整角色设定板（character turnaround and expression sheet），规整多格排版；"
                    "不要左右二分构图，不要只生成头像和单张全身像。第一排放同一角色三视图：正面、严格侧面、背面，均为从头到鞋子的全身站立视图；"
                    "第二排六个等尺寸的表情特写：自然、微笑、悲伤、惊讶、生气、委屈；第三排四个全身动作：行走、奔跑或抬手、开心互动、害羞遮脸。"
                    "所有格子保持同一张脸、发型、妆容、体型、服装和配饰；灰色摄影棚背景，柔和均匀布光，边界清晰，人物不重叠、不裁切、不变形，无文字、水印或多余人物。"
                ),
                "scene": (
                    "保持空间结构清晰、主体建筑或环境可识别，画面完整，"
                    "适合作为短剧场景素材参考图。"
                ),
                "prop": (
                    "主体道具清晰完整，材质、纹理和关键特征明确，画面干净，"
                    "适合作为短剧道具素材参考图。"
                ),
            }.get(asset_type, "生成清晰、可复用的素材设定图。")
        asset_prompt = str(asset.get("prompt") or "").strip()
        theme_prompt = ScriptPlanner._asset_theme_constraint(project.get("theme"), asset_type)
        if theme_prompt in asset_prompt:
            theme_prompt = ""
        return "\n\n".join(
            part for part in (style_prompt, public_prompt, theme_prompt, asset_prompt) if part
        )
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
        result = self._generate_provider_image(
            options, prompt, ratio=str(project.get("ratio") or "9:16")
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
            part
            for part in (
                public_prompt,
                VIDEO_REFERENCE_IMAGE_REVIEW_NOTICE,
                constraint_prompt,
                voice_prompt,
                shot_prompt,
            )
            if part
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
        prompt, reference_images = self._video_generation_inputs(
            project, shot, public_media_base_url
        )
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
