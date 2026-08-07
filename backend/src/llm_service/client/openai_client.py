"""OpenAI Responses API client used by the LLM service layer.

The client keeps the provider-specific protocol here so the application layer
can work with plain text and tool results. ``completion_stream`` yields only
text deltas; function calls are executed internally and the model continues
generating after their results are submitted.
"""

from __future__ import annotations

import inspect
import json
import os
import base64
from collections.abc import AsyncIterator, Mapping
from typing import Any

from openai import AsyncOpenAI, OpenAI
from .openai_types import OpenAICLientBaseOption, OpenAIClientBaseOptions, ToolExecutor


class OpenAICLient:
    """Small stateful wrapper around the synchronous and async OpenAI clients."""

    def __init__(
        self,
        base_options: OpenAIClientBaseOptions | Mapping[str, Any] | None = None,
        *,
        client: AsyncOpenAI | None = None,
        sync_client: OpenAI | None = None,
    ) -> None:
        options = self._read_options(base_options)
        self.model = options["model"] or os.getenv("OPENAI_MODEL", "gpt-4o-mini")
        self.history: list[dict[str, Any]] = list(options["system_messages"])

        client_kwargs: dict[str, Any] = {}
        api_key = options["api_key"] or os.getenv("OPENAI_API_KEY")
        base_url = options["base_url"] or os.getenv("OPENAI_BASE_URL")
        if api_key:
            client_kwargs["api_key"] = api_key
        if base_url:
            client_kwargs["base_url"] = base_url

        if client is None and sync_client is None and not api_key:
            raise ValueError(
                "OpenAI API key is required. Set OPENAI_API_KEY or pass api_key."
            )

        self.client = client or AsyncOpenAI(**client_kwargs)
        self.sync_client = sync_client or OpenAI(**client_kwargs)

    @classmethod
    def create(
        cls,
        base_options: OpenAIClientBaseOptions | Mapping[str, Any] | None = None,
        **kwargs: Any,
    ) -> "OpenAICLient":
        """Create a client with an initialized conversation history."""

        return cls(base_options, **kwargs)

    def generate_image(
        self,
        prompt: str,
        *,
        model: str | None = None,
        size: str = "1024x1024",
        quality: str | None = None,
        n: int = 1,
    ) -> dict[str, Any]:
        """Generate one or more images through the provider's Image API."""

        kwargs: dict[str, Any] = {
            "model": model or self.model,
            "prompt": prompt,
            "size": size,
            "n": n,
        }
        if quality:
            kwargs["quality"] = quality
        response = self.sync_client.images.generate(**kwargs)
        data = getattr(response, "data", None) or []
        if not data:
            raise RuntimeError("图片模型没有返回图片结果")
        item = data[0]
        url = getattr(item, "url", None)
        if url:
            return {"url": str(url), "content": None, "content_type": "image/png"}
        encoded = getattr(item, "b64_json", None)
        if encoded:
            return {
                "url": None,
                "content": base64.b64decode(encoded),
                "content_type": "image/png",
            }
        raise RuntimeError("图片模型没有返回 url 或 b64_json")

    def probe_audio(self, text: str = "模型连接测试", *, voice: str = "alloy") -> None:
        """Send a short speech request to verify an OpenAI-compatible audio model."""

        audio = getattr(self.sync_client, "audio", None)
        speech = getattr(audio, "speech", None) if audio is not None else None
        if speech is None or not hasattr(speech, "create"):
            raise RuntimeError("当前 OpenAI SDK/client 不支持音频生成")
        response = speech.create(
            model=self.model,
            voice=voice,
            input=text,
            response_format="mp3",
        )
        if response is None:
            raise RuntimeError("音频模型没有返回结果")

    def generate_video(
        self,
        prompt: str,
        *,
        model: str | None = None,
        ratio: str = "9:16",
        resolution: str = "720p",
        seconds: int = 8,
        reference_images: list[str] | None = None,
    ) -> dict[str, Any]:
        """Create, poll, and download a video through the Videos API.

        OpenAI's video API is asynchronous.  The SDK's ``create_and_poll``
        helper keeps polling out of the application layer; the completed
        binary is returned so the caller can persist it in its own storage.
        """

        videos = getattr(self.sync_client, "videos", None)
        if videos is None:
            raise RuntimeError("当前 OpenAI SDK/client 不支持视频生成")

        allowed_seconds = (4, 8, 12)
        normalized_seconds = min(allowed_seconds, key=lambda value: abs(value - seconds))
        if resolution == "480p":
            size = "480x854" if ratio == "9:16" else "854x480"
        else:
            size = "720x1280" if ratio == "9:16" else "1280x720"
        video = videos.create_and_poll(
            model=model or self.model,
            prompt=prompt,
            seconds=str(normalized_seconds),
            size=size,
            **(
                {"input_reference": {"image_url": reference_images[0]}}
                if reference_images
                else {}
            ),
        )
        status = getattr(video, "status", None)
        if status != "completed":
            error = getattr(getattr(video, "error", None), "message", None)
            raise RuntimeError(error or f"视频生成失败，状态：{status}")
        video_id = getattr(video, "id", None)
        if not video_id:
            raise RuntimeError("视频模型没有返回任务 ID")
        content = videos.download_content(video_id, variant="video")
        return {
            "url": None,
            "content": content.read(),
            "content_type": "video/mp4",
            "provider_id": str(video_id),
            "seconds": normalized_seconds,
            "size": size,
        }

    # 同步的调用LLM的方法
    def completion(
        self,
        messages: list[Mapping[str, Any]],
        *,
        model: str | None = None,
        tools: list[dict[str, Any]] | None = None,
        tool_executor: ToolExecutor | None = None,
        max_tool_rounds: int = 8,
        timeout: float | None = None,
    ) -> str:
        """Run a synchronous Responses API completion and execute tool calls."""

        self._validate_tool_rounds(max_tool_rounds)
        self.history.extend(self._copy_messages(messages))
        request_tools = self._normalize_tools(tools)
        response_client = self._request_client(self.sync_client, timeout)

        for _ in range(max_tool_rounds + 1):
            response = response_client.responses.create(
                model=model or self.model,
                input=self.history,
                **({"tools": request_tools} if request_tools else {}),
            )
            self.history.extend(self._dump_items(response.output))

            function_calls = [
                item for item in response.output if item.type == "function_call"
            ]
            if not function_calls:
                return response.output_text or ""

            if tool_executor is None:
                raise RuntimeError(
                    "The model requested a function call, but tool_executor was not provided."
                )

            for function_call in function_calls:
                try:
                    arguments = json.loads(function_call.arguments)
                    result = self._invoke_tool_sync(
                        tool_executor, function_call.name, arguments
                    )
                    output = self._serialize_tool_output(result)
                except Exception as exc:  # Return errors to the model for recovery.
                    output = self._serialize_tool_output({"error": str(exc)})

                self.history.append(
                    {
                        "type": "function_call_output",
                        "call_id": function_call.call_id,
                        "output": output,
                    }
                )

        raise RuntimeError("模型工具调用轮数超过限制")

    async def completion_stream(
        self,
        messages: list[Mapping[str, Any]],
        *,
        model: str | None = None,
        tools: list[dict[str, Any]] | None = None,
        tool_executor: ToolExecutor | None = None,
        max_tool_rounds: int = 8,
        timeout: float | None = None,
    ) -> AsyncIterator[str]:
        """Stream text and transparently execute function calls."""

        self._validate_tool_rounds(max_tool_rounds)
        self.history.extend(self._copy_messages(messages))
        request_tools = self._normalize_tools(tools)
        response_client = self._request_client(self.client, timeout)

        for _ in range(max_tool_rounds + 1):
            stream = await response_client.responses.create(
                model=model or self.model,
                input=self.history,
                stream=True,
                **({"tools": request_tools} if request_tools else {}),
            )

            output_items: dict[str, dict[str, Any]] = {}
            output_order: list[str] = []
            partial_calls: dict[str, dict[str, Any]] = {}

            async for event in stream:
                event_type = getattr(event, "type", None)

                if event_type == "response.output_text.delta":
                    yield event.delta
                    continue

                if event_type in {
                    "response.output_item.added",
                    "response.output_item.done",
                }:
                    item = getattr(event, "item", None)
                    if item is not None:
                        self._record_output_item(
                            item, output_items, output_order, partial_calls
                        )
                    continue

                if event_type == "response.function_call_arguments.done":
                    item_id = getattr(event, "item_id", None)
                    if item_id:
                        partial_calls.setdefault(
                            item_id,
                            {
                                "id": item_id,
                                "type": "function_call",
                            },
                        )["arguments"] = event.arguments
                    continue

                # Useful fallback when a provider omits output_item.done but
                # includes the full response in response.completed.
                if event_type == "response.completed":
                    completed = getattr(event, "response", None)
                    for item in getattr(completed, "output", []) or []:
                        self._record_output_item(
                            item, output_items, output_order, partial_calls
                        )

            for item_id, item in partial_calls.items():
                if item_id in output_items:
                    output_items[item_id].update(item)
                else:
                    output_order.append(item_id)
                    output_items[item_id] = item

            response_items = [output_items[item_id] for item_id in output_order]
            self.history.extend(response_items)
            function_calls = [
                item for item in response_items if item.get("type") == "function_call"
            ]

            if not function_calls:
                return

            if tool_executor is None:
                raise RuntimeError(
                    "The model requested a function call, but tool_executor was not provided."
                )

            for function_call in function_calls:
                try:
                    arguments = json.loads(function_call.get("arguments", "{}"))
                    result = await self._invoke_tool_async(
                        tool_executor, function_call["name"], arguments
                    )
                    output = self._serialize_tool_output(result)
                except Exception as exc:  # Return errors to the model for recovery.
                    output = self._serialize_tool_output({"error": str(exc)})

                self.history.append(
                    {
                        "type": "function_call_output",
                        "call_id": function_call["call_id"],
                        "output": output,
                    }
                )

        raise RuntimeError("模型工具调用轮数超过限制")

    @staticmethod
    def _request_client(client: Any, timeout: float | None) -> Any:
        return client.with_options(timeout=timeout) if timeout and hasattr(client, "with_options") else client

    @staticmethod
    def _read_options(
        base_options: OpenAIClientBaseOptions | Mapping[str, Any] | None,
    ) -> dict[str, Any]:
        if base_options is None:
            return {"system_messages": [], "api_key": None, "base_url": None, "model": None}
        if isinstance(base_options, Mapping):
            return {
                "system_messages": list(base_options.get("system_messages", [])),
                "api_key": base_options.get("api_key"),
                "base_url": base_options.get("base_url"),
                "model": base_options.get("model"),
            }
        return {
            "system_messages": list(base_options.system_messages),
            "api_key": base_options.api_key,
            "base_url": base_options.base_url,
            "model": base_options.model,
        }

    @staticmethod
    def _copy_messages(messages: list[Mapping[str, Any]]) -> list[dict[str, Any]]:
        return [dict(message) for message in messages]

    @staticmethod
    def _normalize_tools(
        tools: list[dict[str, Any]] | None,
    ) -> list[dict[str, Any]] | None:
        """Accept both Chat Completions and Responses function schemas."""

        if not tools:
            return None
        normalized: list[dict[str, Any]] = []
        for tool in tools:
            if tool.get("type") == "function" and isinstance(tool.get("function"), Mapping):
                normalized.append({"type": "function", **dict(tool["function"])})
            else:
                normalized.append(dict(tool))
        return normalized

    @staticmethod
    def _dump_items(items: Any) -> list[dict[str, Any]]:
        return [
            item.model_dump(exclude_none=True)
            if hasattr(item, "model_dump")
            else dict(item)
            for item in items
        ]

    @staticmethod
    def _record_output_item(
        item: Any,
        output_items: dict[str, dict[str, Any]],
        output_order: list[str],
        partial_calls: dict[str, dict[str, Any]],
    ) -> None:
        payload = (
            item.model_dump(exclude_none=True)
            if hasattr(item, "model_dump")
            else dict(item)
        )
        item_id = payload.get("id")
        if not item_id:
            return
        if item_id not in output_items:
            output_order.append(item_id)
        output_items[item_id] = payload
        if payload.get("type") == "function_call":
            # Keep the stream item id as the map key. ``call_id`` is the id
            # used to submit the result, while ``id`` identifies the output
            # item and is also used by arguments.done events.
            partial_calls[item_id] = payload

    @staticmethod
    def _serialize_tool_output(result: Any) -> str:
        if isinstance(result, str):
            return result
        return json.dumps(result, ensure_ascii=False, default=str)

    @staticmethod
    def _invoke_tool_sync(
        executor: ToolExecutor,
        name: str,
        arguments: dict[str, Any],
    ) -> Any:
        result = executor(name, arguments)
        if inspect.isawaitable(result):
            raise TypeError("completion() requires a synchronous tool_executor")
        return result

    @staticmethod
    async def _invoke_tool_async(
        executor: ToolExecutor,
        name: str,
        arguments: dict[str, Any],
    ) -> Any:
        result = executor(name, arguments)
        if inspect.isawaitable(result):
            return await result
        return result

    @staticmethod
    def _validate_tool_rounds(max_tool_rounds: int) -> None:
        if max_tool_rounds < 0:
            raise ValueError("max_tool_rounds must be >= 0")
# More idiomatic spelling for new callers; keep the historical misspelling
# above so existing imports do not break.
OpenAIClient = OpenAICLient
