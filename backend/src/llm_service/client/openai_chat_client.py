"""OpenAI Chat Completions adapter for providers without the Responses API."""

from __future__ import annotations

import inspect
import json
import os
from collections.abc import AsyncIterator, Mapping
from typing import Any

from openai import AsyncOpenAI, OpenAI

from .openai_types import OpenAIClientBaseOptions, ToolExecutor


class OpenAIChatClient:
    """Run tool-aware Chat Completions for DashScope and Tencent Hunyuan.

    The language-model planner selects this client for providers whose official
    OpenAI-compatible endpoint exposes ``/chat/completions`` rather than the
    Responses API. It keeps the same completion interface as ``OpenAICLient``
    so agents and screenplay workers do not need provider branches.
    """

    def __init__(self, options: OpenAIClientBaseOptions | Mapping[str, Any], *, client: AsyncOpenAI | None = None, sync_client: OpenAI | None = None) -> None:
        values = self._options(options)
        self.model = str(values["model"] or os.getenv("OPENAI_MODEL", "gpt-4o-mini"))
        self.history: list[dict[str, Any]] = list(values["system_messages"])
        kwargs: dict[str, Any] = {}
        api_key = values["api_key"] or os.getenv("OPENAI_API_KEY")
        base_url = values["base_url"] or os.getenv("OPENAI_BASE_URL")
        if not api_key and client is None and sync_client is None:
            raise ValueError("OpenAI API key is required. Set OPENAI_API_KEY or pass api_key.")
        if api_key:
            kwargs["api_key"] = api_key
        if base_url:
            kwargs["base_url"] = base_url
        self.client = client or AsyncOpenAI(**kwargs)
        self.sync_client = sync_client or OpenAI(**kwargs)

    def completion(self, messages: list[Mapping[str, Any]], *, model: str | None = None, tools: list[dict[str, Any]] | None = None, tool_executor: ToolExecutor | None = None, max_tool_rounds: int = 8, timeout: float | None = None) -> str:
        """Create a non-streaming Chat Completion and execute function tools."""

        self._validate_rounds(max_tool_rounds)
        self.history.extend(self._copy(messages))
        request_tools = self._tools(tools)
        client = self._request_client(self.sync_client, timeout)
        for _ in range(max_tool_rounds + 1):
            response = client.chat.completions.create(model=model or self.model, messages=self.history, **({"tools": request_tools} if request_tools else {}))
            message = response.choices[0].message
            self.history.append(self._dump(message))
            calls = list(getattr(message, "tool_calls", None) or [])
            if not calls:
                return str(getattr(message, "content", None) or "")
            if tool_executor is None:
                raise RuntimeError("The model requested a function call, but tool_executor was not provided.")
            self._append_tool_results(calls, tool_executor)
        raise RuntimeError("模型工具调用轮数超过限制")

    async def completion_stream(self, messages: list[Mapping[str, Any]], *, model: str | None = None, tools: list[dict[str, Any]] | None = None, tool_executor: ToolExecutor | None = None, max_tool_rounds: int = 8, timeout: float | None = None) -> AsyncIterator[str]:
        """Stream Chat Completion text and continue after function tool results."""

        self._validate_rounds(max_tool_rounds)
        self.history.extend(self._copy(messages))
        request_tools = self._tools(tools)
        client = self._request_client(self.client, timeout)
        for _ in range(max_tool_rounds + 1):
            stream = await client.chat.completions.create(model=model or self.model, messages=self.history, stream=True, **({"tools": request_tools} if request_tools else {}))
            text: list[str] = []
            reasoning: list[str] = []
            calls: dict[int, dict[str, Any]] = {}
            async for chunk in stream:
                delta = chunk.choices[0].delta if getattr(chunk, "choices", None) else None
                content = getattr(delta, "content", None)
                if content:
                    text.append(str(content))
                    yield str(content)
                reasoning_content = getattr(delta, "reasoning_content", None)
                if reasoning_content:
                    reasoning.append(str(reasoning_content))
                for part in getattr(delta, "tool_calls", None) or []:
                    index = int(getattr(part, "index", 0))
                    call = calls.setdefault(index, {"id": "", "type": "function", "function": {"name": "", "arguments": ""}})
                    if getattr(part, "id", None):
                        call["id"] = part.id
                    function = getattr(part, "function", None)
                    if function is not None:
                        if getattr(function, "name", None):
                            call["function"]["name"] = function.name
                        if getattr(function, "arguments", None):
                            call["function"]["arguments"] += function.arguments
            serialized_calls = list(calls.values())
            self.history.append({"role": "assistant", "content": "".join(text) or None, **({"reasoning_content": "".join(reasoning)} if reasoning else {}), **({"tool_calls": serialized_calls} if serialized_calls else {})})
            if not serialized_calls:
                return
            if tool_executor is None:
                raise RuntimeError("The model requested a function call, but tool_executor was not provided.")
            await self._append_tool_results_async(serialized_calls, tool_executor)
        raise RuntimeError("模型工具调用轮数超过限制")

    def _append_tool_results(self, calls: list[Any], executor: ToolExecutor) -> None:
        for call in calls:
            function = getattr(call, "function", None)
            name = str(getattr(function, "name", ""))
            arguments = self._arguments(getattr(function, "arguments", "{}"))
            try:
                result = executor(name, arguments)
                if inspect.isawaitable(result):
                    raise TypeError("completion() requires a synchronous tool_executor")
                content = self._serialize(result)
            except Exception as exc:
                content = self._serialize({"error": str(exc)})
            self.history.append({"role": "tool", "tool_call_id": str(getattr(call, "id", "")), "content": content})

    async def _append_tool_results_async(self, calls: list[dict[str, Any]], executor: ToolExecutor) -> None:
        for call in calls:
            try:
                result = executor(str(call["function"]["name"]), self._arguments(call["function"]["arguments"]))
                if inspect.isawaitable(result):
                    result = await result
                content = self._serialize(result)
            except Exception as exc:
                content = self._serialize({"error": str(exc)})
            self.history.append({"role": "tool", "tool_call_id": str(call["id"]), "content": content})

    @staticmethod
    def _options(options: OpenAIClientBaseOptions | Mapping[str, Any]) -> dict[str, Any]:
        if isinstance(options, Mapping):
            return {"api_key": options.get("api_key"), "base_url": options.get("base_url"), "model": options.get("model"), "system_messages": list(options.get("system_messages", []))}
        return {"api_key": options.api_key, "base_url": options.base_url, "model": options.model, "system_messages": list(options.system_messages)}

    @staticmethod
    def _tools(tools: list[dict[str, Any]] | None) -> list[dict[str, Any]] | None:
        normalized: list[dict[str, Any]] = []
        for tool in tools or []:
            if tool.get("type") != "function":
                continue
            function = tool.get("function")
            if isinstance(function, Mapping):
                payload = dict(function)
            else:
                payload = {key: value for key, value in tool.items() if key != "type"}
            # Providers advertise OpenAI compatibility, but do not all accept
            # the Responses-only strict flag used by the original client.
            payload.pop("strict", None)
            normalized.append({"type": "function", "function": payload})
        return normalized or None

    @staticmethod
    def _copy(messages: list[Mapping[str, Any]]) -> list[dict[str, Any]]:
        return [dict(message) for message in messages]

    @staticmethod
    def _dump(value: Any) -> dict[str, Any]:
        return value.model_dump(exclude_none=True) if hasattr(value, "model_dump") else dict(value)

    @staticmethod
    def _arguments(value: Any) -> dict[str, Any]:
        parsed = json.loads(str(value or "{}"))
        return parsed if isinstance(parsed, dict) else {}

    @staticmethod
    def _serialize(value: Any) -> str:
        return value if isinstance(value, str) else json.dumps(value, ensure_ascii=False, default=str)

    @staticmethod
    def _request_client(client: Any, timeout: float | None) -> Any:
        return client.with_options(timeout=timeout) if timeout and hasattr(client, "with_options") else client

    @staticmethod
    def _validate_rounds(value: int) -> None:
        if value < 0:
            raise ValueError("max_tool_rounds must be >= 0")
