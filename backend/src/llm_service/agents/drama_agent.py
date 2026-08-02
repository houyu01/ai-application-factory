"""Agent for long-form drama generation."""

from __future__ import annotations

from typing import ClassVar

from .base_agent import BaseAgent


class DramaAgent(BaseAgent):
    """Load only the skills under the drama domain."""

    skill_directories: ClassVar[list[str]] = [
        "backend/src/llm_service/skills/drama/*"
    ]

    @property
    def agent_name(self) -> str:
        return "drama"
