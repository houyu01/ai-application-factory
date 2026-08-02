"""Agent for interactive full-motion-video game generation."""

from __future__ import annotations

from typing import ClassVar

from .base_agent import BaseAgent


class InteractiveGameAgent(BaseAgent):
    skill_directories: ClassVar[list[str]] = [
        "backend/src/llm_service/skills/interactive_game/*"
    ]

    @property
    def agent_name(self) -> str:
        return "interactive_game"
