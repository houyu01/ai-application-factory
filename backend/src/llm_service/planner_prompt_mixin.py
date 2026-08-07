"""Short-drama orchestration built on the discovered drama skills."""

from __future__ import annotations

import json
import math
import os
import re
from typing import Any

from .agents.drama_agent import DramaAgent
from .client.openai_client import OpenAICLient, OpenAIClientBaseOptions
from .shot_reference_matcher import ShotReferenceMatcher


def _script_planner():
    from .planner import ScriptPlanner
    return ScriptPlanner

class ScriptPlannerPromptMixin:
    """Behavior slice of _script_planner()."""

    @staticmethod
    def rich_prompt_to_text(nodes: list[dict[str, Any]]) -> str:
        parts: list[str] = []
        for node in nodes:
            if node.get("type") == "reference":
                mention_number = int(node.get("mention_number") or 1)
                label = str(node.get("label") or "素材")
                parts.append(f"@图{mention_number}（{label}）")
            else:
                parts.append(str(node.get("text") or ""))
        return "".join(parts).strip()

    @staticmethod
    def _fallback_shot_prompt_rich(
        project: dict[str, Any], shot: dict[str, Any], assets: list[dict[str, Any]],
        template_version: str = "v1",
    ) -> list[dict[str, Any]]:
        selected = _script_planner()._select_shot_reference_assets(shot, assets)
        scenes = [asset for asset in selected if asset.get("type") == "scene"]
        characters = [asset for asset in selected if asset.get("type") == "character"]
        props = [asset for asset in selected if asset.get("type") == "prop"]
        nodes: list[dict[str, Any]] = []

        _script_planner()._append_reference_section(nodes, "场景", scenes)
        _script_planner()._append_reference_section(nodes, "角色", characters + props)

        style = str(project.get("style") or "真人写实风格")
        lighting = "白日户外柔和自然光，地面无明显树影畸变"
        nodes.append({"type": "text", "text": f"风格：{style}，细节丰富\n光线：{lighting}\n位置："})
        if characters:
            nodes.append(_script_planner()._reference_node(characters[0]))
            nodes.append({"type": "text", "text": "位于画面中心，双臂抱着"})
        if props:
            nodes.append(_script_planner()._reference_node(props[0]))
            nodes.append({"type": "text", "text": "，行走于"})
        if scenes:
            nodes.append(_script_planner()._reference_node(scenes[0]))
            nodes.append({"type": "text", "text": "的人行街道上\n"})
        else:
            nodes.append({"type": "text", "text": "画面中心，动作与环境保持连续。\n"})

        action = str(shot.get("original_text") or shot.get("title") or "人物完成当前分镜动作").strip()
        total_duration = max(1, int(shot.get("duration_seconds", shot.get("duration", 10)) or 10))
        if template_version == "v2":
            durations = [total_duration]
        elif total_duration == 1:
            durations = [1]
        elif total_duration == 2:
            durations = [1, 1]
        else:
            first = max(1, total_duration // 3)
            second = max(1, total_duration // 3)
            durations = [first, second, max(1, total_duration - first - second)]
        camera_descriptions = (
            ("中景，平视，跟随人物移动", action),
            (
                "近景，微推近",
                f"{characters[0].get('name', '人物') if characters else '人物'}侧脸转向身旁，动作细节自然延续，"
                f"{props[0].get('name', '道具') if props else '手中物件'}保持在动作关系中",
            ),
            (
                "中景，固定机位",
                f"{characters[0].get('name', '人物') if characters else '人物'}完成当前动作，情绪和画面状态自然收束，"
                "保持前后镜头的空间与姿态连续",
            ),
        )[: len(durations)]
        for index, (duration, (camera, description)) in enumerate(
            zip(durations, camera_descriptions), start=1
        ):
            nodes.append(
                {
                    "type": "text",
                    "text": f"【镜头{index} | 时长{duration}s | 时间：日 外】{camera}｜光线：{lighting}，{description}；",
                }
            )
            if characters:
                nodes.append(_script_planner()._reference_node(characters[0]))
            if props:
                nodes.append({"type": "text", "text": "与"})
                nodes.append(_script_planner()._reference_node(props[0]))
            if scenes:
                nodes.append({"type": "text", "text": "位于"})
                nodes.append(_script_planner()._reference_node(scenes[0]))
            nodes.append({"type": "text", "text": "中。\n"})

            voice_ids = "、".join(
                str(asset.get("voice_name") or "不设置") for asset in characters
            ) or "不设置"
            dialogue = action if index == 1 else "（无新增台词）"
            nodes.append(
                {
                    "type": "text",
                    "text": (
                        f"【配音：旁白｜VoiceID：{voice_ids}｜状态：平稳讲述｜情绪：平静亲和｜"
                        f"语气特点：语速适中，吐字清晰｜台词：{dialogue}】\n"
                    ),
                }
            )
        constraints = project.get("shot_constraints") or {}
        constraint_parts = [
            "需要字幕" if constraints.get("subtitles") else "",
            "需要背景音乐" if constraints.get("background_music") else "",
        ]
        constraints_text = "；".join(part for part in constraint_parts if part)
        if constraints_text:
            nodes.append({"type": "text", "text": f"约束：{constraints_text}。"})
        return _script_planner()._normalize_rich_prompt(nodes, assets)

    @staticmethod
    def _select_shot_reference_assets(
        shot: dict[str, Any], assets: list[dict[str, Any]]
    ) -> list[dict[str, Any]]:
        """Select semantic references while keeping one stable asset per role."""

        by_id = {str(asset.get("id")): asset for asset in assets if asset.get("id")}
        selected: list[dict[str, Any]] = []
        selected_ids: set[str] = set()

        def add(asset: dict[str, Any] | None) -> None:
            if not asset:
                return
            asset_id = str(asset.get("id") or "")
            if asset_id and asset_id not in selected_ids:
                selected_ids.add(asset_id)
                selected.append(asset)

        for node in shot.get("prompt_rich") or []:
            if isinstance(node, dict):
                add(by_id.get(str(node.get("asset_id") or "")))

        haystack = " ".join(
            str(shot.get(key) or "") for key in ("title", "original_text", "prompt")
        )
        for asset in assets:
            name = str(asset.get("name") or "")
            terms = [
                term
                for term in re.split(r"[·/／、,，:：()（）\s]+", name)
                if len(term) >= 2
            ]
            if name and (name in haystack or any(term in haystack for term in terms)):
                add(asset)
        for asset_type in ("scene", "character", "prop"):
            if not any(asset.get("type") == asset_type for asset in selected):
                add(next((asset for asset in assets if asset.get("type") == asset_type), None))
        return selected

    @staticmethod
    def _is_structured_shot_prompt(nodes: list[dict[str, Any]]) -> bool:
        """Require the stable multi-camera sections used by the rich editor."""

        text = _script_planner().rich_prompt_to_text(nodes)
        required_sections = ("场景：", "角色：", "风格：", "光线：", "位置：")
        return all(section in text for section in required_sections) and "【镜头1" in text

    def generate_shot_prompt(
        self,
        project: dict[str, Any],
        shot: dict[str, Any],
        assets: list[dict[str, Any]],
        options: dict[str, Any] | None = None,
    ) -> str:
        return self.rich_prompt_to_text(
            self.generate_shot_prompt_rich(project, shot, assets, options=options)
        )

    @staticmethod
    def _reference_node(asset: dict[str, Any]) -> dict[str, Any]:
        return {
            "type": "reference",
            "asset_id": str(asset.get("id") or ""),
            "asset_type": str(asset.get("type") or "prop"),
            "label": str(asset.get("name") or "素材"),
        }

    @staticmethod
    def _normalize_rich_prompt(
        nodes: Any, assets: list[dict[str, Any]]
    ) -> list[dict[str, Any]]:
        if not isinstance(nodes, list):
            return []
        assets_by_id = {
            str(asset.get("id")): asset for asset in assets if asset.get("id")
        }
        normalized: list[dict[str, Any]] = []
        mention_number = 0
        mention_numbers_by_asset: dict[str, int] = {}
        for raw_node in nodes:
            if not isinstance(raw_node, dict):
                continue
            node_type = str(raw_node.get("type") or "text")
            if node_type == "text":
                text = str(raw_node.get("text") or "")
                if text:
                    normalized.append({"type": "text", "text": text})
                continue
            if node_type != "reference":
                continue
            asset_id = str(raw_node.get("asset_id") or "")
            asset = assets_by_id.get(asset_id)
            asset_type = str(raw_node.get("asset_type") or (asset or {}).get("type") or "prop")
            if not asset and asset_type != "placeholder":
                continue
            if asset_id not in mention_numbers_by_asset:
                mention_number += 1
                mention_numbers_by_asset[asset_id] = mention_number
            normalized.append(
                {
                    "type": "reference",
                    "asset_id": asset_id,
                    "asset_type": asset_type,
                    "label": str(raw_node.get("label") or (asset or {}).get("name") or "占位图"),
                    "mention_number": mention_numbers_by_asset[asset_id],
                    "image_url": (asset or {}).get("image_url"),
                }
            )
        return normalized

    @staticmethod
    def select_ready_shot_reference_assets(
        shot: dict[str, Any], assets: list[dict[str, Any]]
    ) -> list[dict[str, Any]]:
        """Select generated material images that semantically match the current shot script."""

        return ShotReferenceMatcher.select(shot, assets)

    @staticmethod
    def ensure_shot_references(
        nodes: list[dict[str, Any]], assets: list[dict[str, Any]]
    ) -> list[dict[str, Any]]:
        """Append missing automatic matches so each selected image is cited in the prompt."""

        referenced_ids = {
            str(node.get("asset_id") or "")
            for node in nodes
            if isinstance(node, dict) and node.get("type") == "reference"
        }
        missing = [asset for asset in assets if str(asset.get("id") or "") not in referenced_ids]
        if not missing:
            return nodes
        appended: list[dict[str, Any]] = [{"type": "text", "text": "\n自动匹配参考图："}]
        for index, asset in enumerate(missing):
            if index:
                appended.append({"type": "text", "text": "、"})
            appended.append(_script_planner()._reference_node(asset))
        appended.append({"type": "text", "text": "\n"})
        return _script_planner()._normalize_rich_prompt([*nodes, *appended], assets)

    @staticmethod
    def _remove_disallowed_subtitle_sections(
        nodes: list[dict[str, Any]], subtitles_enabled: bool
    ) -> list[dict[str, Any]]:
        """Remove generated subtitle-only blocks when the project disables subtitles.

        Prompt generation invokes this after an LLM response so a provider that
        ignores a constraint cannot leave a subtitle instruction in the stored
        rich prompt. Dialogue and voice blocks remain available for narration.
        """

        if subtitles_enabled:
            return nodes
        cleaned: list[dict[str, Any]] = []
        bracket_pattern = re.compile(r"【\s*字幕(?:说明|要求|内容|标记)?[^】]*】\s*")
        line_pattern = re.compile(
            r"(?m)^\s*字幕(?:说明|要求|内容|标记)?\s*[：:][^\n]*(?:\n|$)"
        )
        constraint_pattern = re.compile(
            r"(^|[：；;，,])\s*(?:不需要|不要|无|无需|禁止)?\s*"
            r"字幕(?:说明|要求|内容|标记)?\s*(?:[；;，,]|$)",
            flags=re.MULTILINE,
        )
        for node in nodes:
            if node.get("type") != "text":
                cleaned.append(node)
                continue
            text = str(node.get("text") or "")
            text = bracket_pattern.sub("", text)
            text = line_pattern.sub("", text)
            text = constraint_pattern.sub(lambda match: match.group(1), text)
            if text:
                cleaned.append({**node, "text": text})
        return cleaned

    @staticmethod
    def _remove_disallowed_music_sections(
        nodes: list[dict[str, Any]], background_music_enabled: bool
    ) -> list[dict[str, Any]]:
        """Remove generated music-only blocks when a project disables music.

        Voice, dialogue, sound effects, and ambient sound are intentionally
        preserved because the project setting controls background music only.
        """

        if background_music_enabled:
            return nodes
        cleaned: list[dict[str, Any]] = []
        label = r"(?:背景音乐|配乐|BGM)"
        bracket_pattern = re.compile(rf"【\s*{label}[^】]*】\s*", flags=re.IGNORECASE)
        line_pattern = re.compile(
            rf"(?im)^\s*{label}\s*[：:][^\n]*(?:\n|$)"
        )
        inline_pattern = re.compile(
            rf"([｜|：；;，,])\s*(?:不需要|不要|无|无需|禁止)?\s*"
            rf"{label}(?:说明|要求|内容)?\s*(?:[｜|；;，,]|$)",
            flags=re.IGNORECASE,
        )
        for node in nodes:
            if node.get("type") != "text":
                cleaned.append(node)
                continue
            text = str(node.get("text") or "")
            text = bracket_pattern.sub("", text)
            text = line_pattern.sub("", text)
            text = inline_pattern.sub(lambda match: match.group(1), text)
            if text:
                cleaned.append({**node, "text": text})
        return cleaned

    @staticmethod
    def _append_reference_section(
        nodes: list[dict[str, Any]], label: str, assets: list[dict[str, Any]]
    ) -> None:
        nodes.append({"type": "text", "text": f"{label}："})
        if not assets:
            nodes.append({"type": "text", "text": "暂无可用素材\n"})
            return
        for index, asset in enumerate(assets):
            if index:
                nodes.append({"type": "text", "text": "。"})
            nodes.append(_script_planner()._reference_node(asset))
        nodes.append({"type": "text", "text": "\n"})

    def generate_shot_prompt_rich(
        self,
        project: dict[str, Any],
        shot: dict[str, Any],
        assets: list[dict[str, Any]],
        options: dict[str, Any] | None = None,
    ) -> list[dict[str, Any]]:
        runtime = {**self.options, **(options or {})}
        template_version = str(runtime.get("prompt_template_version") or "v1")
        constraints = project.get("shot_constraints") or {}
        subtitles_enabled = bool(constraints.get("subtitles"))
        background_music_enabled = bool(constraints.get("background_music"))
        fallback = self._fallback_shot_prompt_rich(
            project, shot, assets, template_version=template_version
        )
        agent = self._agent(runtime, f"{project.get('name', '')}:{shot.get('id', '')}")
        if agent is None:
            return fallback

        asset_text = "\n".join(
            f"- asset_id={asset.get('id', '')}｜type={asset.get('type', 'prop')}｜"
            f"name={asset.get('name', '')}｜prompt={asset.get('prompt', '')}｜"
            f"voice={asset.get('voice_name') or '不设置'}｜voice_prompt={asset.get('voice_prompt') or '无'}｜"
            f"image_url={asset.get('image_url') or '无'}"
            for asset in assets
        ) or "无可用素材"
        skill_context = agent.execute_skill(
            "shot_prompt_generator",
            {
                "project_name": project.get("name", "短剧"),
                "shot_title": shot.get("title", "分镜"),
                "storyboard_text": shot.get("original_text", ""),
                "assets": asset_text,
                "style": project.get("style", "真人风格"),
                "ratio": project.get("ratio", "9:16"),
                "duration": int(shot.get("duration", 10) or 10),
                "resolution": project.get("resolution", "720p"),
                "shot_constraints": project.get("shot_constraints") or {},
                "prompt_template_version": template_version,
            },
        )
        response = agent.completion(
            [
                {
                    "role": "user",
                    "content": (
                        f"模板版本：{template_version}\n"
                        f"模板规则：{runtime.get('prompt_template', '')}\n"
                        "根据以下 Skill 执行结果生成分镜富文本 Prompt。只返回合法 JSON，不要 Markdown。\n"
                        "格式必须是：{\"nodes\":[{\"type\":\"text\",\"text\":\"...\"},"
                        "{\"type\":\"reference\",\"asset_id\":\"素材 catalog 中的 asset_id\","
                        "\"asset_type\":\"character|scene|prop|placeholder\",\"label\":\"素材名称\"}]}。\n"
                        "文字必须严格按以下顺序组织：场景、角色、风格、光线、位置、"
                        f"{'一个连续长镜头' if template_version == 'v2' else '2～3个连续镜头'}、每个镜头对应的配音；"
                        "每个镜头必须使用‘【镜头N | 时长Xs | 时间：日 外】’开头，并在镜头后紧跟‘【配音：旁白｜VoiceID：...｜状态：...｜情绪：...｜语气特点：...｜台词：...】’；"
                        "场景、角色和道具引用必须放在对应段落以及实际动作发生的位置；"
                        "需要使用图片的地方必须输出 reference 节点，不能把图片 URL 写进文字。"
                        "候选 catalog 只包含当前剧本匹配且已生成图片的素材；每项候选素材都必须输出 reference 引用，禁止虚构素材。\n"
                        f"{'需要输出字幕内容。' if subtitles_enabled else '项目不要字幕：不要输出字幕段落、字幕说明、字幕标记或“不要字幕”文字；保留配音内容。'}\n"
                        f"{'需要输出背景音乐说明。' if background_music_enabled else '项目不要音乐：不要输出背景音乐、配乐、BGM 段落或“不要背景音乐”文字；保留配音、音效与环境音。'}\n"
                        f"Skill：{json.dumps(skill_context, ensure_ascii=False)}\n"
                        f"短剧：{project.get('name', '短剧')}\n"
                        f"分镜标题：{shot.get('title', '分镜')}\n"
                        f"分镜原文：{shot.get('original_text', '')}\n"
                        f"已保存素材：\n{asset_text}\n"
                        f"风格：{project.get('style', '真人风格')}；画幅：{project.get('ratio', '9:16')}；"
                        f"分辨率：{project.get('resolution', '720p')}；"
                        f"分镜约束：{json.dumps(project.get('shot_constraints') or {}, ensure_ascii=False)}"
                    ),
                }
            ],
            model=runtime.get("model"),
        )
        parsed = self._parse_json(response)
        normalized = self._normalize_rich_prompt(parsed.get("nodes"), assets)
        normalized = self._remove_disallowed_subtitle_sections(
            normalized, subtitles_enabled
        )
        normalized = self._remove_disallowed_music_sections(
            normalized, background_music_enabled
        )
        return normalized if self._is_structured_shot_prompt(normalized) else fallback
