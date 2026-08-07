"""Semantic selection of ready material images for shot-prompt regeneration."""

from __future__ import annotations

import re
from typing import Any


class ShotReferenceMatcher:
    """Find ready material images for the shot-prompt regeneration flow.

    ``TaskService.run_shot_prompt`` calls this class before asking the planner
    for a new rich prompt. It owns the boundary between an editable storyboard
    script and reusable image assets: only successfully generated character,
    scene, prop, and composite-placeholder images can become prompt references.
    """

    _SUPPORTED_TYPES = frozenset({"character", "scene", "prop", "placeholder"})
    _COMMON_TERMS = frozenset(
        {"人物", "角色", "场景", "画面", "镜头", "图片", "素材", "生成", "参考", "风格"}
    )

    @classmethod
    def select(
        cls, shot: dict[str, Any], assets: list[dict[str, Any]]
    ) -> list[dict[str, Any]]:
        """Return generated assets whose metadata is relevant to this shot's script."""

        source = cls._source_text(shot)
        if not source:
            return []
        source_terms = cls._terms(source)
        scored = [
            (cls._score(asset, source, source_terms), index, asset)
            for index, asset in enumerate(assets)
            if cls._is_ready_reference(asset)
        ]
        return [
            asset for score, _index, asset in sorted(scored, key=lambda item: (-item[0], item[1]))
            if score > 0
        ]

    @staticmethod
    def _source_text(shot: dict[str, Any]) -> str:
        """Use the current editable script, never the previous generated prompt."""

        return " ".join(
            str(shot.get(key) or "").strip()
            for key in ("title", "original_text")
            if str(shot.get(key) or "").strip()
        )

    @classmethod
    def _is_ready_reference(cls, asset: dict[str, Any]) -> bool:
        """Accept only images that can immediately be supplied to a video model."""

        if asset.get("type") not in cls._SUPPORTED_TYPES:
            return False
        if asset.get("status") != "生成成功" or not str(asset.get("image_url") or "").strip():
            return False
        metadata = asset.get("metadata") or {}
        return asset.get("type") != "placeholder" or metadata.get("render_mode") == "generated_composite"

    @classmethod
    def _score(
        cls, asset: dict[str, Any], source: str, source_terms: set[str]
    ) -> int:
        """Score name, description, and placeholder context without sticky old references."""

        name = str(asset.get("name") or "").strip()
        metadata = asset.get("metadata") or {}
        details = " ".join(
            part
            for part in (
                name,
                str(asset.get("prompt") or ""),
                str(metadata.get("scene_name") or ""),
            )
            if part
        )
        if not details:
            return 0
        score = 100 if name and name in source else 0
        name_terms = cls._terms(name)
        detail_terms = cls._terms(details)
        score += 12 * len(name_terms & source_terms)
        score += min(24, 3 * len((detail_terms - name_terms) & source_terms))
        return score

    @classmethod
    def _terms(cls, value: str) -> set[str]:
        """Create short Chinese and alphanumeric terms suitable for local matching."""

        terms: set[str] = set()
        for token in re.findall(r"[\u4e00-\u9fff]{2,}|[A-Za-z0-9_]{2,}", value.lower()):
            if re.fullmatch(r"[\u4e00-\u9fff]+", token):
                for size in range(2, min(4, len(token)) + 1):
                    terms.update(token[index:index + size] for index in range(len(token) - size + 1))
            else:
                terms.add(token)
        return {term for term in terms if term not in cls._COMMON_TERMS}
