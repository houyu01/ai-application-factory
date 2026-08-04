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

class TaskServiceAssetMixin:
    """Behavior slice of TaskService."""

    @staticmethod
    def _placeholder_prompt(
        project: dict[str, Any],
        scene: dict[str, Any],
        placements: list[dict[str, Any]],
    ) -> str:
        position_lines = []
        for index, placement in enumerate(placements):
            position_lines.append(
                f"角色{index + 1}位于画面 x={placement['x']:.2f}, y={placement['y']:.2f}, "
                f"宽度={placement['width']:.2f}, 高度={placement['height']:.2f}，"
                f"动作/备注：{placement.get('note') or '站立'}"
            )
        return "\n".join(
            [
                "生成一张用于视频生成模型参考的占位布局图。",
                f"场景：{scene.get('name', '未命名场景')}；场景提示词：{scene.get('prompt', '')}",
                f"风格：{project.get('style', '真人风格')}；画幅：{project.get('ratio', '9:16')}",
                "请用清晰的方框和字母标记角色在场景中的相对位置，不要改变场景结构。",
                *position_lines,
            ]
        )

    @staticmethod
    def _normalize_placeholder_placements(
        placements: list[dict[str, Any]],
    ) -> list[dict[str, Any]]:
        normalized: list[dict[str, Any]] = []
        for raw in placements:
            asset_id = str(raw.get("asset_id") or "").strip()
            if not asset_id:
                continue
            try:
                width = min(1.0, max(0.04, float(raw.get("width", 0.2))))
                height = min(1.0, max(0.04, float(raw.get("height", 0.35))))
                x = min(1.0 - width, max(0.0, float(raw.get("x", 0.28))))
                y = min(1.0 - height, max(0.0, float(raw.get("y", 0.26))))
            except (TypeError, ValueError):
                width, height, x, y = 0.2, 0.35, 0.28, 0.26
            normalized.append(
                {
                    "id": str(raw.get("id") or f"placement_{len(normalized) + 1}"),
                    "asset_id": asset_id,
                    "x": x,
                    "y": y,
                    "width": width,
                    "height": height,
                    "pose": str(raw.get("pose") or "").strip(),
                    "note": str(raw.get("note") or raw.get("pose") or "").strip(),
                }
            )
        return normalized[:30]

    @staticmethod
    def _render_placeholder_layout(
        scene_bytes: bytes,
        placements: list[dict[str, Any]],
        ratio: str,
    ) -> bytes:
        width, height = (720, 1280) if ratio == "9:16" else (1280, 720)
        with Image.open(BytesIO(scene_bytes)) as source:
            canvas = ImageOps.fit(
                source.convert("RGB"),
                (width, height),
                method=Image.Resampling.LANCZOS,
                centering=(0.5, 0.5),
            )
        draw = ImageDraw.Draw(canvas, "RGBA")
        for index, placement in enumerate(placements):
            x = round(placement["x"] * width)
            y = round(placement["y"] * height)
            box_width = max(24, round(placement["width"] * width))
            box_height = max(24, round(placement["height"] * height))
            right = min(width - 1, x + box_width)
            bottom = min(height - 1, y + box_height)
            draw.rectangle(
                (x, y, right, bottom),
                fill=(249, 115, 22, 35),
                outline=(249, 115, 22, 255),
                width=max(3, round(width / 320)),
            )
            label = chr(65 + index % 26)
            label_top = max(0, y - 30)
            draw.rectangle((x, label_top, x + 30, label_top + 30), fill=(249, 115, 22, 255))
            draw.text((x + 9, label_top + 6), label, fill=(255, 255, 255, 255))
        output = BytesIO()
        canvas.save(output, format="JPEG", quality=92, subsampling=0)
        return output.getvalue()

    @staticmethod
    def _read_media_bytes(url: str) -> bytes:
        if url.startswith("/api/media/"):
            media_id = url.removeprefix("/api/media/").split("?", 1)[0]
            path = media_store.path_for(media_id)
            if path is None:
                raise RuntimeError("当前媒体存储无法读取场景图片")
            return path.read_bytes()
        parsed = urlparse(url)
        if parsed.scheme not in {"http", "https"}:
            raise RuntimeError("场景图片地址无效")
        request = Request(url, headers={"User-Agent": "ai-application-factory/1.0"})
        with urlopen(request, timeout=60) as response:  # noqa: S310 - configured media URL
            return response.read()

    def run_asset_variant_image(
        self, task_id: str, project_id: str, asset_id: str, variant_id: str
    ) -> None:
        try:
            project = self.get_project(project_id)
            asset = self.repository.get_asset(project_id, asset_id)
            if asset is None:
                raise KeyError(f"Asset not found: {asset_id}")
            variant = next(
                (
                    item
                    for item in asset.get("variants", [])
                    if str(item.get("id")) == variant_id
                ),
                None,
            )
            if variant is None:
                raise KeyError(f"Asset variant not found: {variant_id}")
            variant_asset = {
                **asset,
                "prompt": "\n\n".join(
                    part
                    for part in (
                        str(asset.get("prompt") or "").strip(),
                        str(variant.get("prompt") or "").strip(),
                    )
                    if part
                ),
            }
            image_url = self._generate_image_url(project, variant_asset)
            updated = self.repository.update_asset_variant_status(
                project_id,
                asset_id,
                variant_id,
                GenerationStatus.SUCCEEDED,
                image_url=image_url,
            )
            updated_variant = next(
                item for item in updated.get("variants", []) if str(item.get("id")) == variant_id
            )
            self.repository.update_task_status(
                task_id,
                GenerationStatus.SUCCEEDED,
                result={
                    "asset_id": asset_id,
                    "variant_id": variant_id,
                    "image_url": image_url,
                    "prompt": self._asset_generation_prompt(project, variant_asset),
                    "variant": updated_variant,
                },
            )
        except Exception as exc:
            self.repository.update_asset_variant_status(
                project_id, asset_id, variant_id, GenerationStatus.FAILED
            )
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message=str(exc)
            )

    def enqueue_asset_variant_image(
        self, project_id: str, asset_id: str, variant_id: str
    ) -> dict[str, Any]:
        project = self.repository.get_drama(project_id)
        asset = self.repository.get_asset(project_id, asset_id)
        if project is None:
            raise KeyError(f"Project not found: {project_id}")
        if asset is None:
            raise KeyError(f"Asset not found: {asset_id}")
        variant = next(
            (item for item in asset.get("variants", []) if str(item.get("id")) == variant_id),
            None,
        )
        if variant is None:
            raise KeyError(f"Asset variant not found: {variant_id}")
        active_task = self.repository.get_active_task(
            project_id, "asset_variant_image", variant_id
        )
        if active_task is not None:
            return {**active_task, "_reused": True}
        task = self.repository.create_task(
            project_id,
            "asset_variant_image",
            variant_id,
            input_snapshot={
                "project_id": project_id,
                "asset_id": asset_id,
                "variant_id": variant_id,
                "type": "asset_variant_image",
            },
        )
        self.repository.update_asset_variant_status(
            project_id, asset_id, variant_id, GenerationStatus.GENERATING
        )
        return {
            **self.repository.update_task_status(task["id"], GenerationStatus.GENERATING),
            "_reused": False,
        }

    def enqueue_placeholder_image(
        self,
        project_id: str,
        shot_id: str,
        scene_asset_id: str,
        placements: list[dict[str, Any]],
    ) -> dict[str, Any]:
        project = self.get_project(project_id)
        shot = self.repository.get_shot(project_id, shot_id)
        scene = self.repository.get_asset(project_id, scene_asset_id)
        if shot is None:
            raise KeyError(f"Shot not found: {shot_id}")
        if scene is None:
            raise KeyError(f"Scene asset not found: {scene_asset_id}")
        if scene.get("type") != "scene":
            raise ValueError("占位图必须使用场景素材作为背景")
        if not scene.get("image_url"):
            raise ValueError("请先生成场景图片，再创建占位图")
        normalized_placements = self._normalize_placeholder_placements(placements)
        if not normalized_placements:
            raise ValueError("请至少添加一个角色到占位图")
        for placement in normalized_placements:
            role = self.repository.get_asset(project_id, placement["asset_id"])
            if role is None or role.get("type") != "character":
                raise ValueError("占位图只能放置已生成的角色素材")
            if not role.get("image_url"):
                raise ValueError(f"角色“{role.get('name', '未命名')}”尚未生成图片")

        active_task = self.repository.get_active_task_by_snapshot(
            project_id, "placeholder_image", "shot_id", shot_id
        )
        if active_task is not None:
            return {**active_task, "_reused": True}

        version = 1 + sum(
            1
            for asset in project.get("assets", [])
            if asset.get("type") == "placeholder"
            and (asset.get("metadata") or {}).get("shot_id") == shot_id
        )
        metadata = {
            "shot_id": shot_id,
            "scene_asset_id": scene_asset_id,
            "scene_name": scene.get("name", "场景"),
            "placements": normalized_placements,
            "version": version,
        }
        prompt = self._placeholder_prompt(project, scene, normalized_placements)
        asset = self.repository.create_asset(
            project_id,
            "placeholder",
            f"{shot.get('title', '分镜')} · 占位图 {version}",
            prompt,
            metadata,
        )
        self.repository.update_asset_status(
            asset["id"], GenerationStatus.GENERATING
        )
        task = self.repository.create_task(
            project_id,
            "placeholder_image",
            asset["id"],
            input_snapshot={
                "project_id": project_id,
                "shot_id": shot_id,
                "asset_id": asset["id"],
                "scene_asset_id": scene_asset_id,
                "placements": normalized_placements,
                "type": "placeholder_image",
            },
        )
        return {
            **self.repository.update_task_status(
                task["id"], GenerationStatus.GENERATING
            ),
            "_reused": False,
        }

    def run_placeholder_image(
        self, task_id: str, project_id: str, asset_id: str
    ) -> None:
        try:
            project = self.get_project(project_id)
            asset = self.repository.get_asset(project_id, asset_id)
            if asset is None:
                raise KeyError(f"Placeholder asset not found: {asset_id}")
            metadata = asset.get("metadata") or {}
            scene = self.repository.get_asset(project_id, str(metadata.get("scene_asset_id") or ""))
            if scene is None or not scene.get("image_url"):
                raise ValueError("占位图的场景图片不可用")
            image = self._render_placeholder_layout(
                self._read_media_bytes(str(scene["image_url"])),
                self._normalize_placeholder_placements(metadata.get("placements") or []),
                str(project.get("ratio") or "9:16"),
            )
            image_url = media_store.save(image, ".jpg", content_type="image/jpeg")
            self.repository.update_asset_status(
                asset_id, GenerationStatus.SUCCEEDED, image_url=image_url
            )
            shot_id = str(metadata.get("shot_id") or "")
            shot = self.repository.get_shot(project_id, shot_id)
            if shot is not None:
                prompt_rich = list(shot.get("prompt_rich") or [])
                if not prompt_rich and shot.get("prompt"):
                    prompt_rich = [{"type": "text", "text": shot["prompt"]}]
                if not any(node.get("asset_id") == asset_id for node in prompt_rich if isinstance(node, dict)):
                    prompt_rich.extend(
                        [
                            {"type": "text", "text": "\n布局参考："},
                            {
                                "type": "reference",
                                "asset_id": asset_id,
                                "asset_type": "placeholder",
                                "label": asset.get("name", "占位图"),
                                "image_url": image_url,
                            },
                        ]
                    )
                    self.repository.update_shot(
                        project_id,
                        shot_id,
                        prompt=ScriptPlanner.rich_prompt_to_text(prompt_rich),
                        prompt_rich=prompt_rich,
                        placeholder_scene_asset_id=str(metadata.get("scene_asset_id") or ""),
                        placeholder_placements=metadata.get("placements") or [],
                        status=GenerationStatus.NOT_GENERATED,
                    )
            self.repository.update_task_status(
                task_id,
                GenerationStatus.SUCCEEDED,
                result={
                    "asset_id": asset_id,
                    "image_url": image_url,
                    "scene_asset_id": metadata.get("scene_asset_id"),
                    "placements": metadata.get("placements") or [],
                },
            )
        except Exception as exc:
            self.repository.update_asset_status(asset_id, GenerationStatus.FAILED)
            self.repository.update_task_status(
                task_id, GenerationStatus.FAILED, error_message=str(exc)
            )
