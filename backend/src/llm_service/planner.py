"""Short-drama orchestration built on the discovered drama skills."""

from __future__ import annotations

import json
import os
import re
from typing import Any

from .agents.drama_agent import DramaAgent
from .client.openai_client import OpenAICLient, OpenAIClientBaseOptions


class ScriptPlanner:
    """Plan drama structure with an OpenAI-compatible model when configured.

    The planner deliberately keeps a local fallback. It makes the UI and
    SQLite workflow usable before a provider is configured, while configured
    projects use ``DramaAgent`` and the runtime drama skills.
    """

    def __init__(self, options: dict[str, Any] | None = None) -> None:
        self.options = dict(options or {})

    def configure(self, options: dict[str, Any] | None = None) -> None:
        self.options = {**self.options, **(options or {})}

    def plan(self, script: str, options: dict[str, Any] | None = None) -> dict[str, Any]:
        runtime = {**self.options, **(options or {})}
        agent = self._agent(runtime, script)
        if agent is None:
            return self._fallback_plan(script, runtime)

        skill_context = agent.execute_skill(
            "script_decomposer",
            {
                "script": script,
                "style": runtime.get("style", "真人风格"),
                "theme": runtime.get("theme", "都市"),
                "ratio": runtime.get("ratio", "9:16"),
            },
        )
        response = agent.completion(
            [
                {
                    "role": "user",
                    "content": self._decomposition_prompt(script, runtime, skill_context),
                }
            ],
            model=runtime.get("model"),
        )
        parsed = self._parse_json(response)
        return self._normalize_plan(parsed, script, runtime)

    def generate_shot_prompt(
        self,
        project: dict[str, Any],
        shot: dict[str, Any],
        assets: list[dict[str, Any]],
        options: dict[str, Any] | None = None,
    ) -> str:
        runtime = {**self.options, **(options or {})}
        fallback = self._fallback_shot_prompt(project, shot, assets)
        agent = self._agent(runtime, f"{project.get('name', '')}:{shot.get('id', '')}")
        if agent is None:
            return fallback

        asset_text = "\n".join(
            f"- {asset.get('type', 'prop')}：{asset.get('name', '')}｜{asset.get('prompt', '')}"
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
            },
        )
        response = agent.completion(
            [
                {
                    "role": "user",
                    "content": (
                        "根据以下 Skill 执行结果生成最终视频 Prompt。只返回 Prompt 正文。\n"
                        f"Skill：{json.dumps(skill_context, ensure_ascii=False)}\n"
                        f"短剧：{project.get('name', '短剧')}\n"
                        f"分镜标题：{shot.get('title', '分镜')}\n"
                        f"分镜原文：{shot.get('original_text', '')}\n"
                        f"已保存素材：\n{asset_text}\n"
                        f"风格：{project.get('style', '真人风格')}；画幅：{project.get('ratio', '9:16')}"
                    ),
                }
            ],
            model=runtime.get("model"),
        )
        return response.strip() or fallback

    def _agent(self, runtime: dict[str, Any], context_value: str) -> DramaAgent | None:
        api_key = runtime.get("api_key") or os.getenv("OPENAI_API_KEY")
        if not api_key:
            return None
        client = OpenAICLient(
            OpenAIClientBaseOptions(
                api_key=api_key,
                base_url=runtime.get("endpoint") or runtime.get("base_url") or os.getenv("OPENAI_BASE_URL"),
                model=runtime.get("model") or os.getenv("OPENAI_MODEL", "gpt-4o-mini"),
            )
        )
        return DramaAgent(llm_client=client, context={"drama": context_value})

    @staticmethod
    def _decomposition_prompt(script: str, runtime: dict[str, Any], skill_context: dict[str, Any]) -> str:
        return (
            "你正在执行短剧初始化。请只返回合法 JSON，不要 Markdown。\n"
            "JSON 结构必须为：{\"episodes\":[{\"name\":\"第1集\",\"shots\":["
            "{\"title\":\"...\",\"original_text\":\"...\",\"prompt\":\"...\"}]}],"
            "\"assets\":[{\"type\":\"character|scene|prop\",\"name\":\"...\",\"prompt\":\"...\"}]}。\n"
            "每集至少 2 个分镜；每个分镜是可独立生成视频、又能和相邻镜头衔接的连续动作。"
            "素材只提取剧本真实出现且会复用的角色、场景、道具。\n"
            f"配置：风格={runtime.get('style', '真人风格')}，题材={runtime.get('theme', '都市')}，画幅={runtime.get('ratio', '9:16')}。\n"
            f"Skill 执行结果：{json.dumps(skill_context, ensure_ascii=False)}\n"
            f"剧本：\n{script}"
        )

    @staticmethod
    def _fallback_plan(script: str, runtime: dict[str, Any]) -> dict[str, Any]:
        excerpt = script.strip()[:180]
        return {
            "episodes": [
                {
                    "name": "第1集",
                    "shots": [
                        {
                            "id": "shot_001",
                            "title": "开场建立",
                            "original_text": excerpt,
                            "prompt": f"{runtime.get('style', '真人风格')}，建立镜头，交代故事发生的地点和主要人物：{excerpt}",
                        },
                        {
                            "id": "shot_002",
                            "title": "冲突发生",
                            "original_text": excerpt,
                            "prompt": f"延续上一镜头，主要人物因为故事中的关键事件产生动作和情绪变化：{excerpt}",
                        },
                    ],
                }
            ],
            "assets": [
                {
                    "id": "char_001",
                    "type": "character",
                    "name": "主要角色",
                    "prompt": f"短剧主要角色，风格为{runtime.get('style', '真人风格')}，外观和服饰保持统一，基于剧本：{excerpt}",
                },
                {
                    "id": "scene_001",
                    "type": "scene",
                    "name": "主要场景",
                    "prompt": f"短剧主要场景，画幅{runtime.get('ratio', '9:16')}，包含可持续复用的空间结构和光线：{excerpt}",
                },
                {
                    "id": "prop_001",
                    "type": "prop",
                    "name": "关键道具",
                    "prompt": f"剧本中的关键道具，材质、颜色和识别特征清晰，适合单独生成图片：{excerpt}",
                },
            ],
        }

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

        if not episodes or not assets:
            fallback = ScriptPlanner._fallback_plan(script, runtime)
            episodes = episodes or fallback["episodes"]
            assets = assets or fallback["assets"]
        return {"episodes": episodes, "assets": assets}

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
    def _fallback_shot_prompt(project: dict[str, Any], shot: dict[str, Any], assets: list[dict[str, Any]]) -> str:
        asset_lines = "；".join(f"{item.get('type')}：{item.get('name')}（{item.get('prompt')}）" for item in assets)
        return (
            f"场景：{asset_lines or '按照分镜原文建立环境'}\n"
            f"动作：{shot.get('original_text', '')}\n"
            f"风格：{project.get('style', '真人风格')}，画幅：{project.get('ratio', '9:16')}\n"
            "镜头：保持主体连续，平滑衔接前后镜头；光线和情绪服务于剧情。"
        )
