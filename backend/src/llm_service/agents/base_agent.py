"""Generic agent that loads runtime skills and connects them to an LLM."""

from __future__ import annotations

from abc import ABC, abstractmethod
from collections.abc import AsyncIterator, Mapping
from typing import Any, ClassVar

from ..client.openai_client import OpenAICLient
from ..skills import BaseSkill, SkillContext, SkillLoader


class BaseAgent(ABC):
    """Base class for domain agents.

    By default it scans every immediate directory below
    ``backend/src/llm_service/skills/``. A subclass can override
    ``skill_directories`` with one or more glob expressions.
    """

    skill_directories: ClassVar[list[str]] = [
        "backend/src/llm_service/skills/*"
    ]

    def __init__(
        self,
        *,
        llm_client: OpenAICLient | None = None,
        skill_directories: list[str] | None = None,
        context: dict[str, Any] | None = None,
    ) -> None:
        self.llm_client = llm_client or OpenAICLient()
        self.context = context or {}
        self.skill_loader = SkillLoader(
            skill_directories or self.skill_directories
        )
        self.skills: dict[str, BaseSkill] = self.skill_loader.load()

    @property
    @abstractmethod
    def agent_name(self) -> str:
        """Stable name used in skill execution context and logs."""

    def completion(
        self,
        messages: list[Mapping[str, Any]],
        *,
        model: str | None = None,
        max_tool_rounds: int = 8,
    ) -> str:
        """Call the model with all discovered skills exposed as tools."""

        return self.llm_client.completion(
            self._with_agent_context(messages),
            model=model,
            tools=self.skill_tools,
            tool_executor=self._execute_skill,
            max_tool_rounds=max_tool_rounds,
        )

    async def completion_stream(
        self,
        messages: list[Mapping[str, Any]],
        *,
        model: str | None = None,
        max_tool_rounds: int = 8,
    ) -> AsyncIterator[str]:
        """Stream model text while allowing it to invoke discovered skills."""

        async for chunk in self.llm_client.completion_stream(
            self._with_agent_context(messages),
            model=model,
            tools=self.skill_tools,
            tool_executor=self._execute_skill,
            max_tool_rounds=max_tool_rounds,
        ):
            yield chunk

    @property
    def skill_tools(self) -> list[dict[str, Any]]:
        return [skill.tool_definition() for skill in self.skills.values()]

    def _execute_skill(
        self,
        skill_name: str,
        arguments: dict[str, Any],
    ) -> dict[str, Any]:
        try:
            skill = self.skills[skill_name]
        except KeyError as exc:
            raise ValueError(f"Unknown skill: {skill_name}") from exc
        return skill.execute(
            arguments,
            SkillContext(agent_name=self.agent_name, values=self.context),
        )

    def execute_skill(self, skill_name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        """Execute a discovered skill explicitly before or beside an LLM call.

        Tool calls from the model remain supported through ``completion``. This
        public method is useful for deterministic orchestration: the service
        can select the domain skill first and then give its instruction to the
        model without coupling application code to the private tool loop.
        """

        return self._execute_skill(skill_name, arguments)

    def _with_agent_context(
        self,
        messages: list[Mapping[str, Any]],
    ) -> list[dict[str, Any]]:
        skill_summary = "\n".join(
            f"- {skill.name}: {skill.description}" for skill in self.skills.values()
        )
        system_message = {
            "role": "system",
            "content": (
                f"You are the {self.agent_name} domain agent. "
                "Use the available skills when a task matches them.\n"
                f"Available skills:\n{skill_summary}"
            ),
        }
        return [system_message, *[dict(message) for message in messages]]
