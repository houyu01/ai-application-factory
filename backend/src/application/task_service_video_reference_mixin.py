"""Reference-image planning for durable video-generation tasks."""

from __future__ import annotations

import base64
import re
from typing import Any

from ..infrastructure.media_store import media_store


class TaskServiceVideoReferenceMixin:
    """Prepare provider-safe references for video tasks created by the drama flow.

    ``TaskServiceProjectMixin`` uses this boundary while enqueuing a shot and
    ``TaskServiceWorkerMixin`` uses it again before provider submission. It
    owns reference ordering, the Wan R2V five-image limit, and prompt markers
    so a selected boundary frame is always a normal reference image.
    """

    @classmethod
    def _video_reference_plan(
        cls,
        project: dict[str, Any], shot: dict[str, Any], public_media_base_url: str | None = None
    ) -> tuple[list[str], dict[int, int]]:
        """Resolve unique material images and map each prompt marker to its image.

        A material may be cited several times in a shot.  Those citations keep
        the same ``@图N`` marker, while only the first occurrence contributes a
        provider image.  URL de-duplication also covers distinct assets that
        accidentally point at the same rendered image.
        """

        assets_by_id = {
            str(asset.get("id")): asset
            for asset in project.get("assets", [])
            if asset.get("id")
        }
        references: list[str] = []
        reference_indexes: dict[str, int] = {}
        seen_urls: set[str] = set()
        marker_indexes: dict[int, int] = {}
        nodes = shot.get("prompt_rich") or []
        if not isinstance(nodes, list):
            return references, marker_indexes
        for node in nodes:
            if not isinstance(node, dict) or node.get("type") != "reference":
                continue
            asset_id = str(node.get("asset_id") or "")
            referenced = assets_by_id.get(asset_id, {})
            if node.get("asset_type") == "placeholder" and (referenced.get("metadata") or {}).get("render_mode") != "generated_composite":
                continue
            image_url = media_store.provider_reference_url(
                referenced.get("image_url") or node.get("image_url"),
                public_media_base_url,
            )
            if not isinstance(image_url, str) or not image_url:
                continue
            reference_key = f"asset:{asset_id}" if asset_id else f"url:{image_url}"
            reference_index = reference_indexes.get(reference_key)
            if reference_index is None and image_url not in seen_urls:
                reference_index = len(references) + 1
                reference_indexes[reference_key] = reference_index
                seen_urls.add(image_url)
                references.append(image_url)
            elif reference_index is None:
                reference_index = next(
                    index for index, value in enumerate(references, start=1)
                    if value == image_url
                )
                reference_indexes[reference_key] = reference_index
            try:
                marker_number = int(node.get("mention_number"))
            except (TypeError, ValueError):
                continue
            marker_indexes.setdefault(marker_number, reference_index)
        return references, marker_indexes

    @classmethod
    def _video_reference_images(
        cls, project: dict[str, Any], shot: dict[str, Any], public_media_base_url: str | None = None
    ) -> list[str]:
        """Return each material image once, in first-reference order."""

        references, _ = cls._video_reference_plan(
            project, shot, public_media_base_url
        )
        return references

    @staticmethod
    def _video_boundary_frames(
        shot: dict[str, Any], public_media_base_url: str | None = None
    ) -> dict[str, str]:
        """Resolve saved first/last frames into provider-readable image URLs."""

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

    @staticmethod
    def _wan_reference_image_limit(options: dict[str, Any]) -> int | None:
        """Return Wan R2V's reference-image cap without affecting other models."""

        provider = str(options.get("provider") or "").lower()
        model = str(options.get("model") or "").lower()
        return 5 if provider == "dashscope" and model.startswith("wan2.7-r2v") else None

    @staticmethod
    def _configured_boundary_sides(shot: dict[str, Any]) -> list[str]:
        """Count saved boundary images without writing their data URLs during enqueue."""

        frames = shot.get("first_last_frames") or {}
        if not isinstance(frames, dict):
            return []
        configured: list[str] = []
        for side in ("first", "last"):
            value = frames.get(side)
            image_url = value.get("url") if isinstance(value, dict) else value
            if isinstance(image_url, str) and image_url.strip():
                configured.append(side)
        return configured

    def _video_reference_selection(
        self,
        project: dict[str, Any],
        shot: dict[str, Any],
        options: dict[str, Any],
        public_media_base_url: str | None = None,
    ) -> dict[str, Any] | None:
        """Describe a Wan truncation so the enqueue response can warn the editor."""

        limit = self._wan_reference_image_limit(options)
        if limit is None:
            return None
        boundary_sides = self._configured_boundary_sides(shot)
        reference_count = len(self._video_reference_images(project, shot, public_media_base_url))
        selected_count = max(0, limit - len(boundary_sides))
        ignored_count = max(0, reference_count - selected_count)
        if not ignored_count:
            return None
        frame_label = {
            ("first", "last"): "首尾帧",
            ("first",): "首帧",
            ("last",): "尾帧",
        }.get(tuple(boundary_sides), "")
        selected_description = (
            f"{frame_label} + {selected_count}个参考图"
            if frame_label else f"{selected_count}个参考图"
        )
        return {
            "reference_limit": limit,
            "selected_reference_count": selected_count,
            "ignored_reference_count": ignored_count,
            "boundary_sides": boundary_sides,
            "warning_message": (
                f"由于选择的模型限制，目前只选用了{selected_description}，"
                f"后续的{ignored_count}张参考图未使用，请手动调整。"
            ),
        }

    def _video_generation_inputs(
        self,
        project: dict[str, Any],
        shot: dict[str, Any],
        public_media_base_url: str | None = None,
        reference_limit: int | None = None,
    ) -> tuple[str, list[str]]:
        """Build ordered normal references and prompt-directed boundary controls."""

        asset_images, marker_indexes = self._video_reference_plan(
            project, shot, public_media_base_url
        )
        boundary_frames = self._video_boundary_frames(shot, public_media_base_url)
        boundary_count = len(boundary_frames)
        asset_limit = max(0, reference_limit - boundary_count) if reference_limit is not None else len(asset_images)
        reference_images = asset_images[:asset_limit]
        prompt = self._video_generation_prompt(project, shot)
        prompt = self._remap_reference_markers(prompt, marker_indexes)
        if len(reference_images) < len(asset_images):
            prompt = self._without_unselected_reference_markers(prompt, len(reference_images))
        reference_indexes = {
            image_url: index
            for index, image_url in enumerate(reference_images, start=1)
        }
        frame_instructions: list[str] = []
        for side in ("first", "last"):
            image_url = boundary_frames.get(side)
            if not image_url:
                continue
            reference_index = reference_indexes.get(image_url)
            if reference_index is None:
                reference_index = len(reference_images) + 1
                reference_images.append(image_url)
                reference_indexes[image_url] = reference_index
            if side == "first":
                frame_instructions.append(f"@图{reference_index} 是视频首帧：视频第一帧必须以该图的主体、构图、光线和状态开始。")
            else:
                frame_instructions.append(f"@图{reference_index} 是视频尾帧：视频最后一帧必须收束到该图的主体、构图、光线和状态。")
        if not frame_instructions:
            return prompt, reference_images
        frame_prompt = "首尾帧控制（最高优先级）：输入参考图与 @图编号按相同顺序对应。\n" + "\n".join(frame_instructions)
        return "\n\n".join((prompt, frame_prompt)), reference_images

    @staticmethod
    def _without_unselected_reference_markers(prompt: str, selected_count: int) -> str:
        """Keep omitted reference labels as prose rather than invalid image markers."""

        def replace(match: re.Match[str]) -> str:
            return match.group(0) if int(match.group(1)) <= selected_count else (match.group(2) or "后续参考素材")

        return re.sub(r"@图\s*(\d+)(?:（([^）]*)）)?", replace, prompt)

    @staticmethod
    def _remap_reference_markers(prompt: str, marker_indexes: dict[int, int]) -> str:
        """Make repeated material citations point at their single image input."""

        def replace(match: re.Match[str]) -> str:
            marker_number = int(match.group(1))
            return f"@图{marker_indexes.get(marker_number, marker_number)}"

        return re.sub(r"@图\s*(\d+)", replace, prompt)
