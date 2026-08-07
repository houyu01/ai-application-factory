"""Durable cover-image generation for short-drama projects."""

from __future__ import annotations

import base64
from pathlib import Path
from typing import Any

from ..domain.models import GenerationStatus
from ..infrastructure.media_store import media_store


class TaskServiceCoverMixin:
    """Create and resume cover jobs started from the drama cover dialog.

    The cover panel calls this workflow after the user chooses characters,
    scenes, uploaded references, an aspect ratio, and an output count. It owns
    validation, durable progress through the requested images, and persistence
    of every generated cover in the reusable asset history.
    """

    COVER_RATIOS = {"9:16", "16:9", "1:1", "3:4", "4:3"}

    def enqueue_cover_image(
        self,
        project_id: str,
        *,
        name: str,
        prompt: str,
        ratio: str,
        count: int,
        character_asset_ids: list[str],
        scene_asset_ids: list[str],
        extra_reference_asset_ids: list[str],
    ) -> dict[str, Any]:
        """Persist a cover asset and its restart-safe image-generation task."""

        project = self.repository.get_drama(project_id)
        if project is None:
            raise KeyError(f"Project not found: {project_id}")
        normalized_ratio = ratio if ratio in self.COVER_RATIOS else str(project.get("ratio") or "9:16")
        normalized_count = max(1, min(8, int(count)))
        groups = {
            "character": list(dict.fromkeys(character_asset_ids)),
            "scene": list(dict.fromkeys(scene_asset_ids)),
            "cover_reference": list(dict.fromkeys(extra_reference_asset_ids)),
        }
        assets = {asset["id"]: asset for asset in self.repository.list_assets(project_id)}
        missing: list[str] = []
        reference_ids: list[str] = []
        for expected_type, ids in groups.items():
            for asset_id in ids:
                asset = assets.get(asset_id)
                if asset is None or asset.get("type") != expected_type:
                    raise ValueError(f"封面参考素材不存在或类型不匹配：{asset_id}")
                if not asset.get("image_url"):
                    missing.append(str(asset.get("name") or asset_id))
                reference_ids.append(asset_id)
        if missing:
            raise ValueError("请先生成或上传以下封面参考图：" + "、".join(missing))
        metadata = {
            "ratio": normalized_ratio,
            "count": normalized_count,
            "character_asset_ids": groups["character"],
            "scene_asset_ids": groups["scene"],
            "extra_reference_asset_ids": groups["cover_reference"],
            "reference_asset_ids": reference_ids,
        }
        cover = self.repository.create_asset(
            project_id, "cover", name, prompt, metadata=metadata
        )
        task = self.repository.create_task(
            project_id,
            "cover_image",
            cover["id"],
            input_snapshot={"project_id": project_id, "cover_asset_id": cover["id"], **metadata},
        )
        self.repository.update_asset_status(cover["id"], GenerationStatus.GENERATING)
        task = self.repository.update_task_status(task["id"], GenerationStatus.GENERATING)
        return {"cover": self.repository.get_asset(project_id, cover["id"]), "task": task}

    def run_cover_image(self, task_id: str, project_id: str, cover_id: str) -> None:
        """Generate remaining cover versions and resume after worker restarts."""

        try:
            project = self.get_project(project_id)
            cover = self.repository.get_asset(project_id, cover_id)
            if cover is None or cover.get("type") != "cover":
                raise KeyError(f"Cover asset not found: {cover_id}")
            metadata = cover.get("metadata") or {}
            count = max(1, min(8, int(metadata.get("count") or 1)))
            reference_ids = [str(value) for value in metadata.get("reference_asset_ids") or []]
            assets = {asset["id"]: asset for asset in project.get("assets", [])}
            references = [assets[value] for value in reference_ids if value in assets]
            if len(references) != len(reference_ids) or any(not item.get("image_url") for item in references):
                raise ValueError("封面引用的角色、场景或上传参考图已经缺失")
            generated = list(cover.get("image_history") or [])
            for _index in range(len(generated), count):
                image_url = self._generate_cover_url(project, cover, references)
                self.repository.update_asset_status(
                    cover_id, GenerationStatus.GENERATING, image_url=image_url
                )
                generated.append({"url": image_url})
            self.repository.update_asset_status(cover_id, GenerationStatus.SUCCEEDED)
            self.repository.update_task_status(
                task_id,
                GenerationStatus.SUCCEEDED,
                result={"cover_asset_id": cover_id, "image_urls": [item.get("url") for item in generated]},
            )
        except Exception as exc:
            self.repository.update_asset_status(cover_id, GenerationStatus.FAILED)
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message=str(exc)
            )

    def _generate_cover_url(
        self, project: dict[str, Any], cover: dict[str, Any], references: list[dict[str, Any]]
    ) -> str:
        """Call the configured project image model with cover reference images."""

        options = self._provider_options(project, "multimodal")
        if not options.get("api_key"):
            raise RuntimeError("未配置图像模型 API Key，无法生成封面")
        metadata = cover.get("metadata") or {}
        ratio = str(metadata.get("ratio") or project.get("ratio") or "9:16")
        prompt = self._cover_prompt(project, cover, references, ratio)
        reference_images = [self._cover_reference_input(str(item["image_url"])) for item in references]
        result = self._generate_provider_image(
            options, prompt, ratio=ratio, reference_images=reference_images
        )
        return self._persist_provider_result(result, ".png", "封面图片模型")

    @staticmethod
    def _cover_prompt(
        project: dict[str, Any], cover: dict[str, Any], references: list[dict[str, Any]], ratio: str
    ) -> str:
        reference_text = "、".join(
            f"{item.get('type')}：{item.get('name')}" for item in references
        ) or "无额外参考图"
        user_prompt = str(cover.get("prompt") or "").strip() or "突出核心人物与故事冲突，构图清晰，具有短剧海报传播力。"
        return (
            f"为短剧《{cover.get('name') or project.get('name')}》生成一张 {ratio} 封面海报。\n"
            f"整体风格：{project.get('style') or '真人风格'}；背景主题：{project.get('theme') or '都市'}。\n"
            f"参考素材：{reference_text}。必须保持参考人物脸部、服装与场景特征一致。\n"
            f"用户补充要求：{user_prompt}\n"
            "画面完整、主体突出、视觉层级清晰，不生成水印、Logo、错误肢体或无关文字。"
        )

    @staticmethod
    def _cover_reference_input(image_url: str) -> str:
        media_id = image_url.rstrip("/").rsplit("/", 1)[-1]
        local_path = media_store.path_for(media_id)
        if local_path and Path(local_path).exists():
            encoded = base64.b64encode(Path(local_path).read_bytes()).decode("ascii")
            return f"data:{media_store.content_type(local_path)};base64,{encoded}"
        return image_url

    @staticmethod
    def _cover_ark_size(ratio: str) -> str:
        return {"9:16": "1440x2560", "16:9": "2560x1440", "1:1": "2048x2048", "3:4": "1728x2304", "4:3": "2304x1728"}.get(ratio, "1440x2560")

    @staticmethod
    def _cover_openai_size(ratio: str) -> str:
        if ratio in {"9:16", "3:4"}:
            return "1024x1536"
        if ratio in {"16:9", "4:3"}:
            return "1536x1024"
        return "1024x1024"
