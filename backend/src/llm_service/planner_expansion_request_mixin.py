"""Provider request and cancellation helpers for screenplay expansion."""

from __future__ import annotations

import asyncio
from collections.abc import Callable
from contextlib import suppress
from time import monotonic, sleep
from typing import Any

from . import planner_expansion_mixin
from .planner_expansion_mixin import RetryableExpansionError


WEB_SEARCH_TOOLS: list[dict[str, str]] = [{"type": "web_search"}]
STANDARD_SLEEP = sleep


class ExpansionCancelledError(RuntimeError):
    """Signal that a creator stopped an active screenplay expansion."""


class ScriptPlannerExpansionRequestMixin:
    """Own provider calls for the screenplay-expansion worker flow.

    ``ScriptPlannerExpansionMixin`` uses this class while producing the story
    bible and each screenplay installment. It owns cooperative cancellation of
    an async provider stream, allowing the durable worker slot to return as
    soon as the creator clicks the dialog's cancel button.
    """

    CANCELLATION_POLL_SECONDS = 0.1

    @staticmethod
    def _raise_if_expansion_cancelled(
        is_cancelled: Callable[[], bool] | None,
    ) -> None:
        """Stop work before another provider request or persistence callback."""

        if is_cancelled and is_cancelled():
            raise ExpansionCancelledError("剧本扩写已取消")

    def _build_expansion_outline(
        self,
        agent: Any,
        source_excerpt: str,
        runtime: dict[str, Any],
        framework_research: str = "",
        stream: Callable[[str], None] | None = None,
        is_cancelled: Callable[[], bool] | None = None,
    ) -> str:
        """Produce a compact story bible that keeps installment prompts coherent."""

        self._raise_if_expansion_cancelled(is_cancelled)
        minimum_chars, maximum_chars = self.expansion_char_limits(runtime)
        premise_context = agent.execute_skill(
            "premise_expander",
            {
                "premise": source_excerpt,
                "genre": str(runtime.get("theme") or "短剧"),
                "target_audience": "短剧观众",
                "episode_count": self._expansion_episode_count(runtime),
                "target_min_chars": minimum_chars,
                "target_max_chars": maximum_chars,
            },
        )
        self._raise_if_expansion_cancelled(is_cancelled)
        bible_context = agent.execute_skill(
            "story_bible_generator",
            {
                "premise": source_excerpt,
                "expanded_concept": str(premise_context.get("instruction") or ""),
                "episode_count": self._expansion_episode_count(runtime),
                "format_requirements": self._story_bible_format_requirements(runtime),
            },
        )
        messages = [{
            "role": "user",
            "content": (
                "请为长篇短剧扩写建立紧凑故事圣经和分集推进表。保留原稿中的明确人物、事件、"
                "设定和情感走向；补齐连续的冲突、反转、伏笔和结局，不要写正文剧本。\n"
                f"目标剧集数：{self._expansion_episode_count(runtime)} 集。\n"
                f"完整剧本长度要求：至少 {minimum_chars:,} 字，最多 {maximum_chars:,} 字。\n"
                f"联网同类框架研究（只可借鉴抽象叙事结构，禁止复写作品内容）：\n{framework_research}\n"
                f"创意扩写技能：{premise_context.get('instruction', '')}\n"
                f"故事圣经技能：{bible_context.get('instruction', '')}\n"
                f"原始剧本：\n{source_excerpt}"
            ),
        }]
        if stream:
            response = self._stream_completion_with_retry(
                agent,
                "故事大纲",
                messages,
                runtime,
                stream,
                tools=WEB_SEARCH_TOOLS if runtime.get("enable_web_search") else None,
                is_cancelled=is_cancelled,
            )
        else:
            response = self._completion_with_retry(
                agent,
                "故事大纲",
                messages,
                runtime,
                tools=WEB_SEARCH_TOOLS if runtime.get("enable_web_search") else None,
                is_cancelled=is_cancelled,
            )
        return self._clean_expansion_text(response) or source_excerpt

    def _write_expansion_installment(
        self,
        runtime: dict[str, Any],
        source_excerpt: str,
        outline: str,
        continuity: str,
        installment: int,
        written_chars: int,
        episode_start: int | None = None,
        episode_end: int | None = None,
        target_episode_chars: int | None = None,
        installment_max_chars: int | None = None,
        stream: Callable[[str], None] | None = None,
        is_cancelled: Callable[[], bool] | None = None,
    ) -> str:
        """Generate one screenplay installment without growing client history."""

        self._raise_if_expansion_cancelled(is_cancelled)
        agent = self._agent(runtime, source_excerpt)
        if agent is None:
            raise RuntimeError("语言模型配置在扩写过程中不可用")
        writer_context = agent.execute_skill(
            "script_writer",
            {
                "story_bible": outline[:6_000],
                "episode_card": self._installment_episode_card(
                    installment, written_chars, episode_start, episode_end
                ),
                "scene_plan": "按因果推进剧情，避免总结、重复和跳过关键冲突。",
                "style_requirements": "中文影视剧本格式，包含场景、动作、对白、情绪和结尾钩子。",
            },
        )
        messages = [{
            "role": "user",
            "content": (
                "请直接续写长篇短剧正文，不要解释、不要写创作说明。必须是具体场景、"
                "动作、对白和情绪推进，而不是梗概或重复前文。\n"
                f"{self._installment_format_requirements(installment, episode_start, episode_end, target_episode_chars, installment_max_chars)}\n"
                f"写作技能：{writer_context.get('instruction', '')}\n"
                f"故事圣经：\n{outline[:6_000]}\n"
                f"原始剧本：\n{source_excerpt}\n"
                f"上一节末尾（仅用于衔接）：\n{continuity or '这是开篇，请从原稿事件自然展开。'}"
            ),
        }]
        if stream:
            response = self._stream_completion_with_retry(
                agent,
                f"扩写剧本第 {installment} 节",
                messages,
                runtime,
                stream,
                tools=WEB_SEARCH_TOOLS if runtime.get("enable_web_search") else None,
                is_cancelled=is_cancelled,
            )
        else:
            response = self._completion_with_retry(
                agent,
                f"扩写剧本第 {installment} 节",
                messages,
                runtime,
                tools=WEB_SEARCH_TOOLS if runtime.get("enable_web_search") else None,
                is_cancelled=is_cancelled,
            )
        return self._clean_expansion_text(response)

    def _completion_with_retry(
        self,
        agent: Any,
        stage: str,
        messages: list[dict[str, str]],
        runtime: dict[str, Any],
        tools: list[dict[str, str]] | None = None,
        is_cancelled: Callable[[], bool] | None = None,
    ) -> str:
        """Retry temporary provider failures while honoring cancellation."""

        for attempt in range(1, self.EXPANDED_SCRIPT_MAX_RETRIES + 1):
            self._raise_if_expansion_cancelled(is_cancelled)
            try:
                kwargs: dict[str, Any] = {"model": runtime.get("model")}
                if tools:
                    kwargs["tools"] = tools
                kwargs["timeout"] = self._request_timeout(runtime)
                return agent.completion(messages, **kwargs)
            except ExpansionCancelledError:
                raise
            except Exception as exc:
                retryable = self._is_retryable_provider_error(exc)
                if attempt == self.EXPANDED_SCRIPT_MAX_RETRIES or not retryable:
                    error_type = RetryableExpansionError if retryable else RuntimeError
                    raise error_type(
                        f"{stage}请求语言模型失败（已尝试 {attempt} 次）：{str(exc) or exc.__class__.__name__}"
                    ) from exc
                self._wait_for_expansion_retry(2 ** (attempt - 1), is_cancelled)
        raise AssertionError("扩写模型重试循环未返回结果")

    def _stream_completion_with_retry(
        self,
        agent: Any,
        stage: str,
        messages: list[dict[str, str]],
        runtime: dict[str, Any],
        on_delta: Callable[[str], None],
        tools: list[dict[str, str]] | None = None,
        is_cancelled: Callable[[], bool] | None = None,
    ) -> str:
        """Stream one installment and close its provider connection on cancel."""

        for attempt in range(1, self.EXPANDED_SCRIPT_MAX_RETRIES + 1):
            fragments: list[str] = []

            async def collect() -> None:
                kwargs: dict[str, Any] = {"model": runtime.get("model")}
                if tools:
                    kwargs["tools"] = tools
                kwargs["timeout"] = self._request_timeout(runtime)
                stream = agent.completion_stream(messages, **kwargs)
                next_delta: asyncio.Task[Any] | None = None
                try:
                    while True:
                        self._raise_if_expansion_cancelled(is_cancelled)
                        next_delta = asyncio.create_task(anext(stream))
                        while not next_delta.done():
                            await asyncio.wait(
                                {next_delta}, timeout=self.CANCELLATION_POLL_SECONDS
                            )
                            self._raise_if_expansion_cancelled(is_cancelled)
                        try:
                            delta = next_delta.result()
                        except StopAsyncIteration:
                            return
                        fragments.append(str(delta))
                        on_delta("".join(fragments))
                        next_delta = None
                finally:
                    if next_delta and not next_delta.done():
                        next_delta.cancel()
                        with suppress(asyncio.CancelledError):
                            await next_delta
                    close = getattr(stream, "aclose", None)
                    if callable(close):
                        with suppress(RuntimeError):
                            await close()

            try:
                asyncio.run(collect())
                return "".join(fragments)
            except ExpansionCancelledError:
                raise
            except Exception as exc:
                retryable = self._is_retryable_provider_error(exc)
                if attempt == self.EXPANDED_SCRIPT_MAX_RETRIES or not retryable:
                    error_type = RetryableExpansionError if retryable else RuntimeError
                    raise error_type(
                        f"{stage}请求语言模型失败（已尝试 {attempt} 次）：{str(exc) or exc.__class__.__name__}"
                    ) from exc
                self._wait_for_expansion_retry(2 ** (attempt - 1), is_cancelled)
        raise AssertionError("扩写流式模型重试循环未返回结果")

    def _wait_for_expansion_retry(
        self, seconds: float, is_cancelled: Callable[[], bool] | None
    ) -> None:
        """Wait between retries without delaying a creator-requested cancel."""

        if is_cancelled is None:
            legacy_sleep = getattr(planner_expansion_mixin, "sleep", STANDARD_SLEEP)
            (legacy_sleep if legacy_sleep is not STANDARD_SLEEP else sleep)(seconds)
            return
        deadline = monotonic() + seconds
        while monotonic() < deadline:
            self._raise_if_expansion_cancelled(is_cancelled)
            sleep(min(self.CANCELLATION_POLL_SECONDS, deadline - monotonic()))

    @staticmethod
    def _is_retryable_provider_error(error: Exception) -> bool:
        """Recognize OpenAI-compatible connection, timeout, throttling, and 5xx errors."""

        if error.__class__.__name__ in {
            "APIConnectionError", "APITimeoutError", "ConnectError", "ConnectTimeout",
            "ReadTimeout", "WriteTimeout", "PoolTimeout", "TimeoutError", "ReadError",
            "RemoteProtocolError", "ConnectionError", "RateLimitError",
        }:
            return True
        status_code = getattr(error, "status_code", None)
        return isinstance(status_code, int) and (status_code == 429 or status_code >= 500)

    def _request_timeout(self, runtime: dict[str, Any]) -> float:
        """Bound a provider call so a stalled web search becomes a task error."""

        try:
            value = runtime.get("request_timeout_seconds") or self.EXPANSION_REQUEST_TIMEOUT_SECONDS
            return max(1.0, float(value))
        except (TypeError, ValueError):
            return float(self.EXPANSION_REQUEST_TIMEOUT_SECONDS)
