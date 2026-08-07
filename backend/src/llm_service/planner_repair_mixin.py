"""Short-drama orchestration built on the discovered drama skills."""

from __future__ import annotations

import json
import math
import os
import re
from typing import Any

from .agents.drama_agent import DramaAgent
from .client.openai_client import OpenAICLient, OpenAIClientBaseOptions


def _script_planner():
    from .planner import ScriptPlanner
    return ScriptPlanner

class ScriptPlannerRepairMixin:
    """Normalize provider output and repair incomplete drama plans."""

    @staticmethod
    def _repair_asset_catalog(
        assets: list[dict[str, Any]], script: str, runtime: dict[str, Any]
    ) -> list[dict[str, Any]]:
        """Validate model output and guarantee a multi-asset catalog.

        A model can technically satisfy the JSON schema with one generic asset
        per type, or copy the complete user script into every prompt. Those
        responses are not useful for image generation, so discard the weak
        entries, enrich concise entries, and fill each category from the
        semantic local catalog.
        """

        source = _script_planner()._clean_script(script)
        style = str(runtime.get("style") or "真人风格")
        theme = str(runtime.get("theme") or "都市")
        fallback = _script_planner()._fallback_asset_catalog(source, runtime)
        fallback_by_type = {
            asset_type: [asset for asset in fallback if asset["type"] == asset_type]
            for asset_type in ("character", "scene", "prop")
        }
        generic_names = {
            "character": {"主要角色", "核心角色", "角色", "主角", "人物"},
            "scene": {"主要场景", "核心场景", "场景", "主场景"},
            "prop": {"关键道具", "主要道具", "道具", "核心道具"},
        }
        used_names: set[str] = set()
        repaired: list[dict[str, Any]] = []
        type_counts = {"character": 0, "scene": 0, "prop": 0}

        for raw in assets:
            asset_type = str(raw.get("type") or "prop")
            if asset_type not in type_counts:
                continue
            name = re.sub(r"\s+", " ", str(raw.get("name") or "")).strip()
            prompt = re.sub(r"\s+", " ", str(raw.get("prompt") or "")).strip()
            if (
                not name
                or not prompt
                or name in generic_names[asset_type]
                or _script_planner()._is_full_script_like(prompt, source)
            ):
                continue
            name = _script_planner()._meaningful_asset_name(
                asset_type, name, source, type_counts[asset_type]
            )
            if name in used_names:
                continue
            index = type_counts[asset_type]
            if asset_type == "character":
                prompt = _script_planner()._character_prompt(name, prompt, style, index, theme)
            elif asset_type == "scene":
                prompt = _script_planner()._scene_prompt(
                    name,
                    f"{name}是剧情中反复使用、承载人物行动和信息变化的空间。",
                    prompt,
                    "场景内的桌椅、门窗、灯具和线索物件按剧情状态摆放，保留真实使用痕迹",
                    "色调和光线服务于当前剧情情绪；无人物，无背景文字。",
                    theme,
                )
            else:
                prompt = (
                    f"{_script_planner()._asset_theme_constraint(theme, 'prop')}\n"
                    f"{name}\n颜色、材质与细节：{prompt}\n"
                    "装饰、磨损与表面文字：保持关键纹样、刻字和使用痕迹稳定。\n"
                    "主体道具清晰完整，适合作为短剧道具素材参考图。"
                )
            repaired.append(
                {
                    **raw,
                    "name": name,
                    "prompt": prompt,
                    "id": raw.get("id") or f"{asset_type}_{index + 1:03d}",
                }
            )
            used_names.add(name)
            type_counts[asset_type] += 1

        for asset_type in ("character", "scene", "prop"):
            for fallback_asset in fallback_by_type[asset_type]:
                if type_counts[asset_type] >= 2:
                    break
                if fallback_asset["name"] in used_names:
                    continue
                repaired.append({**fallback_asset})
                used_names.add(fallback_asset["name"])
                type_counts[asset_type] += 1

        return repaired

    @staticmethod
    def _normalize_plan(raw: dict[str, Any], script: str, runtime: dict[str, Any]) -> dict[str, Any]:
        episodes: list[dict[str, Any]] = []
        raw_episodes = raw.get("episodes") if isinstance(raw, dict) else []
        if isinstance(raw_episodes, dict):
            raw_episodes = [raw_episodes]
        for episode_index, raw_episode in enumerate(raw_episodes or [], start=1):
            if not isinstance(raw_episode, dict):
                continue
            shots: list[dict[str, Any]] = []
            raw_shots = raw_episode.get("shots") or raw_episode.get("storyboards") or []
            for shot_index, raw_shot in enumerate(raw_shots, start=1):
                if not isinstance(raw_shot, dict):
                    continue
                original = str(
                    raw_shot.get("original_text")
                    or raw_shot.get("source_segment")
                    or raw_shot.get("script_segment")
                    or raw_shot.get("shot_text")
                    or raw_shot.get("storyboardText")
                    or raw_shot.get("storyboard_text")
                    or raw_shot.get("script")
                    or script[:180]
                ).strip()
                shots.append(
                    {
                        "id": raw_shot.get("id") or f"shot_{episode_index}_{shot_index}",
                        "title": str(raw_shot.get("title") or f"分镜 {shot_index}"),
                        "original_text": original,
                        "prompt": str(raw_shot.get("prompt") or raw_shot.get("promptText") or "").strip(),
                        "duration": raw_shot.get("duration", 10),
                    }
                )
            if shots:
                episodes.append(
                    {
                        "name": str(raw_episode.get("name") or raw_episode.get("title") or f"第{episode_index}集"),
                        "shots": shots,
                    }
                )

        assets: list[dict[str, Any]] = []
        raw_assets = raw.get("assets") if isinstance(raw, dict) else []
        for asset_index, raw_asset in enumerate(raw_assets or [], start=1):
            if not isinstance(raw_asset, dict):
                continue
            type_name = str(raw_asset.get("type") or raw_asset.get("kind") or "prop").lower()
            type_name = {"role": "character", "scene": "scene", "prop": "prop"}.get(type_name, type_name)
            if type_name not in {"character", "scene", "prop"}:
                continue
            name = str(raw_asset.get("name") or raw_asset.get("title") or f"素材 {asset_index}").strip()
            prompt = str(raw_asset.get("prompt") or raw_asset.get("promptText") or raw_asset.get("description") or "").strip()
            if name and prompt:
                assets.append({"id": raw_asset.get("id") or f"asset_{asset_index}", "type": type_name, "name": name, "prompt": prompt})

        planner = _script_planner()
        assets = planner._repair_asset_catalog(assets, script, runtime)
        # Long-form batches already own their episode boundaries and shot text.
        # Applying the legacy short-script repair here would redistribute the
        # complete 50-episode screenplay across all shots and destroy that plan.
        if episodes and not planner._is_long_form_screenplay(script, runtime):
            episodes = planner._repair_shot_segments(episodes, script, runtime)
        if not episodes:
            fallback = _script_planner()._fallback_plan(script, runtime)
            episodes = episodes or fallback["episodes"]
        return {"episodes": episodes, "assets": assets}
