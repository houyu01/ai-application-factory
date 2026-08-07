"""Long-form screenplay expansion orchestration before decomposition."""

from __future__ import annotations

import math
import re
from collections.abc import Callable
from time import sleep
from typing import Any


def _script_planner():
    from .planner import ScriptPlanner

    return ScriptPlanner


class RetryableExpansionError(RuntimeError):
    """A temporary language-provider failure that a durable task may resume from."""


class ScriptPlannerExpansionMixin:
    """Expand an input premise into the screenplay consumed by decomposition.

    ``TaskServiceDecompositionMixin`` calls this mixin before it requests
    episodes, shots, and assets. Provider requests live in the companion
    request mixin, while this class owns progression and checkpoints.
    """

    EXPANDED_SCRIPT_TARGET_CHARS = 5_000
    EXPANDED_SCRIPT_MAX_CHARS = 10_000
    EXPANDED_SCRIPT_CHUNK_CHARS = 6_000
    EXPANDED_SCRIPT_MAX_CHUNKS = 30
    EXPANDED_SCRIPT_MAX_RETRIES = 3
    EXPANSION_REQUEST_TIMEOUT_SECONDS = 120

    def expand_script(
        self,
        script: str,
        options: dict[str, Any] | None = None,
        progress: Callable[[int, int], None] | None = None,
        on_stage: Callable[[str], None] | None = None,
        existing_script: str = "",
        existing_outline: str = "",
        checkpoint: Callable[[str, int, int], None] | None = None,
        outline_checkpoint: Callable[[str], None] | None = None,
        stream: Callable[[str], None] | None = None,
        is_cancelled: Callable[[], bool] | None = None,
    ) -> str | None:
        """Generate a resumable screenplay while checking cancellation often."""

        self._raise_if_expansion_cancelled(is_cancelled)
        source = _script_planner()._clean_script(script)
        if not source:
            raise ValueError("剧本内容不能为空")
        runtime = {**self.options, **(options or {})}
        minimum_chars, maximum_chars = self.expansion_char_limits(runtime)
        target_episode_count = self._target_episode_count(runtime)
        existing = self._clean_expansion_text(existing_script)
        long_form = self._requires_long_form_expansion(runtime)
        if long_form and existing and not self._is_resumable_long_form_checkpoint(existing):
            existing = ""
        if len(existing) > maximum_chars:
            existing = ""
        if len(existing) >= minimum_chars and (
            not long_form or self._episode_count(existing) >= target_episode_count
        ):
            return existing
        if minimum_chars <= len(source) <= maximum_chars and not existing and (
            not long_form or self._episode_count(source) >= target_episode_count
        ):
            return source
        if long_form:
            runtime.setdefault("request_timeout_seconds", self.EXPANSION_REQUEST_TIMEOUT_SECONDS)
        outline_agent = self._agent(runtime, source)
        if outline_agent is None:
            if long_form:
                raise RuntimeError(
                    f"未配置可调用的语言模型，无法执行包含 web_search 的 {target_episode_count} 集长剧扩写。"
                    "请先在配置页保存语言模型的 endpoint、API Key 和可选模型。"
                )
            return None
        source_excerpt = self._expansion_source_excerpt(source)
        outline = self._clean_expansion_text(existing_outline)
        framework_research = self._expand_framework_research(
            long_form, bool(runtime.get("enable_web_search", False)), outline,
            outline_agent, source_excerpt, runtime, on_stage, is_cancelled,
        )
        self._raise_if_expansion_cancelled(is_cancelled)
        if outline:
            if long_form and on_stage:
                on_stage(f"已读取已保存的 {target_episode_count} 集故事圣经")
        else:
            if long_form and on_stage:
                on_stage(f"正在生成 {target_episode_count} 集故事圣经")
            outline = self._build_expansion_outline(
                outline_agent, source_excerpt, runtime, framework_research,
                stream=stream, is_cancelled=is_cancelled,
            )
            if outline_checkpoint:
                self._raise_if_expansion_cancelled(is_cancelled)
                outline_checkpoint(outline)
        chunks = [existing] if existing else []
        expanded = "\n\n".join(chunks).strip()
        total_chars = sum(map(len, chunks))
        first_installment = self._first_expansion_installment(
            expanded, total_chars, long_form
        )
        for installment in range(
            first_installment, first_installment + self.EXPANDED_SCRIPT_MAX_CHUNKS
        ):
            self._raise_if_expansion_cancelled(is_cancelled)
            if total_chars >= minimum_chars and (
                not long_form or self._episode_count("\n\n".join(chunks)) >= target_episode_count
            ):
                break
            chunk = self._expand_installment(
                runtime, source_excerpt, outline, chunks, installment, total_chars,
                minimum_chars, maximum_chars, long_form, stream, is_cancelled,
            )
            self._raise_if_expansion_cancelled(is_cancelled)
            if len(chunk) < 200:
                raise RuntimeError(f"扩写剧本第 {installment} 节没有返回有效内容")
            chunks.append(chunk)
            total_chars += len(chunk)
            expanded = "\n\n".join(chunks).strip()
            if checkpoint:
                self._raise_if_expansion_cancelled(is_cancelled)
                checkpoint(expanded, total_chars, minimum_chars)
            if progress:
                self._raise_if_expansion_cancelled(is_cancelled)
                progress(total_chars, minimum_chars)
        expanded = "\n\n".join(chunks).strip()
        self._validate_expanded_script(
            expanded, minimum_chars, maximum_chars, long_form, target_episode_count
        )
        return expanded

    def continue_expanded_script(
        self,
        script: str,
        expanded_script: str,
        options: dict[str, Any] | None = None,
        on_stage: Callable[[str], None] | None = None,
        existing_outline: str = "",
        checkpoint: Callable[[str, int, int], None] | None = None,
        outline_checkpoint: Callable[[str], None] | None = None,
        stream: Callable[[str], None] | None = None,
        is_cancelled: Callable[[], bool] | None = None,
    ) -> str:
        """Append one new screenplay installment to a creator-approved draft.

        The screenplay dialog calls this after the initial decomposition has
        completed. Unlike ``expand_script``, it always makes a new provider
        request instead of treating the configured minimum length as complete.
        """

        self._raise_if_expansion_cancelled(is_cancelled)
        source = _script_planner()._clean_script(script)
        existing = self._clean_expansion_text(expanded_script)
        if not source:
            raise ValueError("剧本内容不能为空")
        if not existing:
            raise ValueError("尚无可继续扩写的剧本内容")
        runtime = {**self.options, **(options or {})}
        _, maximum_chars = self.expansion_char_limits(runtime)
        available_chars = maximum_chars - len(existing) - 2
        if available_chars < 200:
            raise ValueError(f"扩写剧本已达到 {maximum_chars} 字上限，无法继续扩写")
        outline_agent = self._agent(runtime, source)
        if outline_agent is None:
            raise RuntimeError("未配置可调用的语言模型，无法继续扩写剧本。")
        source_excerpt = self._expansion_source_excerpt(source)
        outline = self._clean_expansion_text(existing_outline)
        if not outline:
            if on_stage:
                on_stage("正在生成续写所需的故事圣经")
            research = self._expand_framework_research(
                self._requires_long_form_expansion(runtime),
                bool(runtime.get("enable_web_search", False)),
                outline,
                outline_agent,
                source_excerpt,
                runtime,
                on_stage,
                is_cancelled,
            )
            outline = self._build_expansion_outline(
                outline_agent, source_excerpt, runtime, research,
                is_cancelled=is_cancelled,
            )
            if outline_checkpoint:
                self._raise_if_expansion_cancelled(is_cancelled)
                outline_checkpoint(outline)
        if on_stage:
            on_stage("正在继续扩写剧本")
        target_chars = min(
            maximum_chars,
            len(existing) + 2 + min(self.EXPANDED_SCRIPT_CHUNK_CHARS, available_chars),
        )

        def stream_continuation(delta: str) -> None:
            if stream:
                stream(self._streamed_expansion_preview([existing], delta, maximum_chars))

        chunk = self._write_expansion_installment(
            runtime,
            source_excerpt,
            outline,
            existing[-2_400:],
            self._first_expansion_installment(existing, len(existing), False),
            len(existing),
            target_episode_chars=self.EXPANDED_SCRIPT_CHUNK_CHARS,
            installment_max_chars=available_chars,
            stream=stream_continuation if stream else None,
            is_cancelled=is_cancelled,
        )
        self._raise_if_expansion_cancelled(is_cancelled)
        chunk = self._fit_installment_within_limit(
            chunk, available_chars, None, None
        )
        if len(chunk) < 200:
            raise RuntimeError("继续扩写没有返回有效内容")
        continued = f"{existing}\n\n{chunk}".strip()
        if checkpoint:
            self._raise_if_expansion_cancelled(is_cancelled)
            checkpoint(continued, len(continued), target_chars)
        return continued

    def _expand_framework_research(
        self, long_form: bool, enable_web_search: bool, outline: str, agent: Any,
        source_excerpt: str, runtime: dict[str, Any], on_stage: Callable[[str], None] | None,
        is_cancelled: Callable[[], bool] | None,
    ) -> str:
        """Research comparable abstract structures only when a new outline needs it."""

        if not (long_form and enable_web_search and not outline):
            return ""
        if on_stage:
            on_stage(f"正在联网研究同类故事框架（0/{len(self.LONG_FORM_RESEARCH_TOPICS)}）")
        return self._research_story_frameworks(
            agent, source_excerpt, runtime, on_stage=on_stage, is_cancelled=is_cancelled
        )

    def _first_expansion_installment(
        self, expanded: str, total_chars: int, long_form: bool
    ) -> int:
        """Choose the first unpersisted screenplay installment number."""

        if long_form:
            return max(
                1,
                math.ceil(self._episode_count(expanded) / self.LONG_FORM_EPISODES_PER_INSTALLMENT) + 1,
            )
        return max(1, math.ceil(total_chars / self.EXPANDED_SCRIPT_CHUNK_CHARS) + 1)

    def _expand_installment(
        self, runtime: dict[str, Any], source_excerpt: str, outline: str,
        chunks: list[str], installment: int, total_chars: int, minimum_chars: int,
        maximum_chars: int, long_form: bool, stream: Callable[[str], None] | None,
        is_cancelled: Callable[[], bool] | None,
    ) -> str:
        """Request, cap, and return a single continuation installment."""

        joined = "\n\n".join(chunks)
        episode_start = self._next_episode_number(joined) if long_form else None
        episode_end = (
            min(self._target_episode_count(runtime), episode_start + self.LONG_FORM_EPISODES_PER_INSTALLMENT - 1)
            if episode_start is not None else None
        )
        limit = self._installment_character_limit(
            "\n\n".join(chunks).strip(), maximum_chars, episode_start, episode_end,
            self._target_episode_count(runtime),
        )
        on_delta = None
        if stream:
            on_delta = lambda delta: stream(
                self._streamed_expansion_preview(chunks, delta, maximum_chars)
            )
        chunk = self._write_expansion_installment(
            runtime, source_excerpt, outline, "\n".join(chunks[-2:])[-2_400:],
            installment, total_chars, episode_start=episode_start, episode_end=episode_end,
            target_episode_chars=(math.ceil(minimum_chars / self._target_episode_count(runtime))
                                  if long_form else self.EXPANDED_SCRIPT_CHUNK_CHARS),
            installment_max_chars=limit, stream=on_delta, is_cancelled=is_cancelled,
        )
        return self._fit_installment_within_limit(chunk, limit, episode_start, episode_end)

    def _validate_expanded_script(
        self, expanded: str, minimum_chars: int, maximum_chars: int, long_form: bool,
        target_episode_count: int,
    ) -> None:
        """Validate the length and long-form episode contract before decomposition."""

        if len(expanded) < minimum_chars:
            raise RuntimeError(f"扩写剧本未达到 {minimum_chars} 字，当前为 {len(expanded)} 字")
        if len(expanded) > maximum_chars:
            raise RuntimeError(f"扩写剧本超过 {maximum_chars} 字上限，当前为 {len(expanded)} 字")
        if long_form and self._episode_count(expanded) < target_episode_count:
            raise RuntimeError(
                f"扩写剧本未拆分到 {target_episode_count} 集，当前为 {self._episode_count(expanded)} 集"
            )

    @staticmethod
    def _streamed_expansion_preview(chunks: list[str], delta: str, maximum_chars: int) -> str:
        """Keep browser previews within the same hard ceiling as checkpoints."""

        prefix = "\n\n".join(chunks).strip()
        joiner = "\n\n" if prefix else ""
        available = max(0, maximum_chars - len(prefix) - len(joiner))
        return f"{prefix}{joiner}{str(delta)[:available]}".strip()

    @staticmethod
    def _expansion_source_excerpt(source: str) -> str:
        """Keep long source material within every provider request's context budget."""

        if len(source) <= 12_000:
            return source
        return f"{source[:9_000]}\n\n……（中间原稿省略）……\n\n{source[-3_000:]}"

    @staticmethod
    def _clean_expansion_text(value: str) -> str:
        """Remove presentation fences while preserving screenplay paragraphs."""

        text = str(value or "").strip()
        text = re.sub(r"^```(?:text|markdown)?\s*|\s*```$", "", text, flags=re.IGNORECASE)
        return text.strip()
