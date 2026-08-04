"""Provider-neutral OpenAI client configuration and tool callback types."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any, TypeAlias


ToolExecutor: TypeAlias = Callable[[str, dict[str, Any]], Any]


@dataclass(slots=True)
class OpenAIClientBaseOptions:
    """Configuration for an ``OpenAICLient`` instance.

    Credentials are intentionally read from options or environment variables;
    API keys must not be committed to the repository.
    """

    system_messages: list[dict[str, Any]] = field(default_factory=list)
    api_key: str | None = None
    base_url: str | None = None
    model: str | None = None


OpenAICLientBaseOption = OpenAIClientBaseOptions
