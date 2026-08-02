"""Contracts shared by all runtime LLM skills."""

from __future__ import annotations

from abc import ABC
from dataclasses import dataclass, field
from typing import Any, ClassVar


@dataclass(slots=True)
class SkillContext:
    """Runtime context passed to a skill when the model invokes it."""

    agent_name: str
    values: dict[str, Any] = field(default_factory=dict)


class BaseSkill(ABC):
    """Base contract for a skill discovered by :class:`SkillLoader`.

    A skill is deliberately small: it describes a capability as a Responses
    API function tool and returns an execution envelope. Domain agents can
    later override ``execute`` when a skill needs persistence, retrieval, or a
    dedicated model call.
    """

    name: ClassVar[str]
    description: ClassVar[str]
    parameters: ClassVar[dict[str, Any]] = {
        "type": "object",
        "properties": {},
        "additionalProperties": False,
    }
    instruction: ClassVar[str] = ""

    def tool_definition(self) -> dict[str, Any]:
        """Return the Responses API function-tool definition."""

        return {
            "type": "function",
            "name": self.name,
            "description": self.description,
            "parameters": self.parameters,
            "strict": True,
        }

    def execute(
        self,
        arguments: dict[str, Any],
        context: SkillContext,
    ) -> dict[str, Any]:
        """Return a normalized skill request for the outer model.

        This default implementation makes skills useful immediately as
        prompt/operator modules. A production skill can override this method
        to call a repository, a queue, or a specialized model provider.
        """

        return {
            "skill": self.name,
            "agent": context.agent_name,
            "instruction": self.instruction,
            "arguments": arguments,
        }
