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

class ScriptPlannerUtilsMixin:
    """Behavior slice of _script_planner()."""

    @staticmethod
    def _character_personality(identity: str, index: int) -> str:
        """Return behavior-level traits instead of a vague personality adjective."""

        if _script_planner()._contains_any(identity, ("反派", "boss", "魁首", "幕后", "对手", "敌人", "执棋")):
            return "表面从容克制，内心多疑且有强烈控制欲，习惯先试探他人再做决定；面对失控会迅速压住情绪并重新布局，很少主动袒露真实目的"
        if _script_planner()._contains_any(identity, ("师父", "道长", "收养", "长辈", "师尊")):
            return "沉稳寡言，习惯把情绪藏在克制的语气里，观察周全后才做判断；对晚辈表面严厉但照顾细致，遇到关键选择时会独自沉思"
        if _script_planner()._contains_any(identity, ("师姐", "师妹", "女主", "爱人", "同行者", "女性", "少女")):
            return "待人温柔但有分寸，习惯先倾听和观察再表达意见；面对危险会保持冷静，对信任的人愿意主动安抚和维护，做决定时不轻易动摇"
        if _script_planner()._contains_any(identity, ("主人公", "男主", "少年", "追查", "真相", "故事主人公")):
            return "性格坚毅但不鲁莽，平时话少而善于观察，习惯先确认细节再行动；遇到线索会反复沉思、追问细节，对信任的人温柔有责任感，遭遇不公时会先压住情绪再果断行动"
        profiles = (
            "性格温和细腻，习惯先观察环境和他人的语气再开口；遇到压力时会短暂沉思，对熟悉的人更愿意耐心解释，行动前会反复确认细节",
            "性格谨慎敏感，习惯把重要信息记在心里并反复推敲；面对陌生人保持礼貌距离，遇到真正重要的选择时会坚定承担后果",
            "性格直率有行动力，想到办法后会立即尝试并根据结果调整；情绪变化会直接写在脸上，但面对亲近的人愿意主动道歉和保护对方",
        )
        return profiles[index % len(profiles)]

    @staticmethod
    def _parse_json(text: str) -> dict[str, Any]:
        cleaned = text.strip()
        if cleaned.startswith("```"):
            cleaned = re.sub(r"^```(?:json)?\s*|\s*```$", "", cleaned, flags=re.IGNORECASE | re.DOTALL)
        try:
            value = json.loads(cleaned)
            return value if isinstance(value, dict) else {}
        except json.JSONDecodeError:
            match = re.search(r"\{.*\}", cleaned, re.DOTALL)
            if not match:
                return {}
            try:
                value = json.loads(match.group(0))
                return value if isinstance(value, dict) else {}
            except json.JSONDecodeError:
                return {}

    @staticmethod
    def _asset_theme_constraint(theme: Any, asset_type: str) -> str:
        """Build the project-era constraint shared by every visual asset prompt."""

        normalized_theme = re.sub(r"\s+", " ", str(theme or "都市")).strip() or "都市"
        focus = {
            "character": "角色身份、发型、妆容、服装与配饰",
            "scene": "建筑、道路、室内陈设、照明、交通工具与环境细节",
            "prop": "道具造型、材质、制作工艺、表面文字与实际功能",
        }.get(asset_type, "全部视觉元素")
        return (
            f"叙述背景主题：{normalized_theme}。{focus}必须符合该主题对应的时代、地域、社会环境与技术水平；"
            "除非剧本明确包含穿越或跨时代设定，否则禁止出现与背景主题不符的元素。"
        )

    @staticmethod
    def _character_prompt(
        name: str, identity: str, style: str, index: int, theme: str = "都市"
    ) -> str:
        identity_context = f"{name} {identity}"
        story_context = _script_planner()._character_story_context(name, identity)
        personality = _script_planner()._character_personality(identity_context, index)
        gender = (
            "青年女性"
            if any(term in identity_context for term in ("女性", "少女", "女主", "师姐", "师妹", "爱人", "姑娘"))
            else "青年男性"
        )
        face = "鹅蛋脸线条柔和，眉眼清澈而警觉" if gender == "青年女性" else "脸型清瘦、棱角分明，眉眼澄澈并带着坚毅感"
        complexion = "肤色白皙自然，面部有少量真实皮肤纹理" if gender == "青年女性" else "肤色是健康的浅蜜色，面部有经历风霜后的真实质感"
        hair = "乌黑长发半束，用素色发簪固定，发尾自然垂落" if gender == "青年女性" else "墨黑长发用素色木簪整齐束起，发丝边缘清晰"
        body = "身形纤细挺拔，肩颈舒展，体态利落" if gender == "青年女性" else "身形挺拔匀称，肩背有长期修炼形成的力量感"
        cloth = "衣料是轻薄但有纹理的青灰色云纱，搭配耐行动的深色腰封" if gender == "青年女性" else "衣料是透气的云纱材质，穿着素色长衫和便于行动的深色护腕"
        return (
            f"{_script_planner()._asset_theme_constraint(theme, 'character')}\n"
            f"身世、身份与性格：{story_context}；性格具体表现为：{personality}。负责推动故事中的关键行动，性格和人物关系保持前后一致。\n"
            f"年龄与外观：{gender}，脸型与五官：{face}；{complexion}；头发：{hair}；身型：{body}；"
            f"服装与衣料：{cloth}。整体按{style}呈现，保持固定脸部特征、发型、体态和服装连续性。"
        )

    @staticmethod
    def _contains_any(text: str, terms: tuple[str, ...]) -> bool:
        return any(term in text for term in terms)

    @staticmethod
    def _scene_prompt(
        name: str, origin: str, appearance: str, objects: str, atmosphere: str,
        theme: str = "都市",
    ) -> str:
        return (
            f"{_script_planner()._asset_theme_constraint(theme, 'scene')}\n"
            f"{name}\n场景由来：{origin}\n外形与色调：{appearance}\n"
            f"场景中物品状态：{objects}\n整体氛围与人物文字限制：{atmosphere}\n"
            "保持空间结构清晰，适合作为短剧场景素材参考图。"
        )

    @staticmethod
    def _character_story_context(name: str, identity: str) -> str:
        """Keep narrative identity while removing duplicated visual sections."""

        context = re.sub(r"\s+", " ", str(identity or "")).strip()
        context = re.sub(r"^身世、身份与性格\s*[:：]\s*", "", context)
        context = re.split(r"\s*(?:年龄与外观|外观与视觉特征|视觉特征)\s*[:：]", context, maxsplit=1)[0]
        context = context.strip(" \n；;。")
        return context or f"{name}是剧情中的重要人物，拥有明确的行动目标和人物关系。"

    @staticmethod
    def _unique_specs(items: list[tuple[Any, ...]]) -> list[tuple[Any, ...]]:
        seen: set[str] = set()
        result: list[tuple[Any, ...]] = []
        for item in items:
            key = str(item[0])
            if key in seen:
                continue
            seen.add(key)
            result.append(item)
        return result
