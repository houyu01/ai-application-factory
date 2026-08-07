"""Short-drama orchestration built on the discovered drama skills."""

from __future__ import annotations

import json
import math
import os
import re
from typing import Any

from .agents.drama_agent import DramaAgent
from .client.openai_client import OpenAICLient, OpenAIClientBaseOptions

from .planner_prompt_mixin import ScriptPlannerPromptMixin
from .planner_decomposition_mixin import ScriptPlannerDecompositionMixin
from .planner_expansion_mixin import ScriptPlannerExpansionMixin
from .planner_expansion_request_mixin import ScriptPlannerExpansionRequestMixin
from .planner_long_form_mixin import ScriptPlannerLongFormMixin
from .planner_repair_mixin import ScriptPlannerRepairMixin
from .planner_utils_mixin import ScriptPlannerUtilsMixin

WEB_SEARCH_TOOLS: list[dict[str, str]] = [{"type": "web_search"}]


class ScriptPlanner(
    ScriptPlannerPromptMixin,
    ScriptPlannerLongFormMixin,
    ScriptPlannerExpansionMixin,
    ScriptPlannerExpansionRequestMixin,
    ScriptPlannerDecompositionMixin,
    ScriptPlannerRepairMixin,
    ScriptPlannerUtilsMixin,
):
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
        if self._is_long_form_screenplay(script, runtime):
            if agent is None:
                return self._fallback_long_form_plan(script, runtime)
            return self._plan_long_form(script, runtime, agent)
        if agent is None:
            return self._fallback_plan(script, runtime)

        skill_context = agent.execute_skill(
            "script_decomposer",
            {
                "script": script,
                "style": runtime.get("style", "真人风格"),
                "theme": runtime.get("theme", "都市"),
                "ratio": runtime.get("ratio", "9:16"),
                "resolution": runtime.get("resolution", "720p"),
                "shot_constraints": runtime.get("shot_constraints", {}),
            },
        )
        asset_skill_context = {
            asset_type: agent.execute_skill(
                "asset_prompt_generator",
                {
                    "asset_type": asset_type,
                    "name": "短剧素材目录",
                    "story_context": script,
                    "style": runtime.get("style", "真人风格"),
                    "theme": runtime.get("theme", "都市"),
                },
            )
            for asset_type in ("character", "scene", "prop")
        }
        response = agent.completion(
            [
                {
                    "role": "user",
                    "content": self._decomposition_prompt(
                        script, runtime, skill_context, asset_skill_context
                    ),
                }
            ],
            model=runtime.get("model"),
            tools=WEB_SEARCH_TOOLS,
        )
        parsed = self._parse_json(response)
        return self._normalize_plan(parsed, script, runtime)
