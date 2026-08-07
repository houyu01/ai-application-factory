"""Long-form episode planning after a screenplay has been expanded."""

from __future__ import annotations

import math
import re
from collections.abc import Callable
from typing import Any


class ScriptPlannerLongFormMixin:
    """Build durable, target-episode storyboard skeletons for new dramas.

    ``TaskServiceDecompositionMixin`` invokes this mixin after ``expand_script``
    has persisted the expanded screenplay. It keeps long-form work in
    bounded ten-episode batches so a provider failure can be diagnosed without
    silently collapsing the project into a handful of generic shots.
    """

    LONG_FORM_MIN_EPISODES = 25
    LONG_FORM_MIN_CHARS = 5_000
    LONG_FORM_BATCH_SIZE = 10
    LONG_FORM_EPISODES_PER_INSTALLMENT = 5
    LONG_FORM_RESEARCH_TOPICS = (
        "主角成长与身份反转的节奏",
        "多集追更钩子与中段升级结构",
        "人物关系、情感线与对手线的交叉推进",
        "终局回收伏笔与成功结局的结构",
    )
    _EPISODE_HEADING = re.compile(
        r"(?m)^\s*[【\[]?\s*第\s*(\d{1,3})\s*集\s*(?:[：:]\s*([^】\]\n]+))?[】\]]?\s*$"
    )

    @classmethod
    def _target_episode_count(cls, runtime: dict[str, Any] | None = None) -> int:
        """Resolve the validated target count for one project's numbered episodes."""

        value = (runtime or {}).get("episode_count", cls.LONG_FORM_MIN_EPISODES)
        try:
            count = int(value)
        except (TypeError, ValueError) as exc:
            raise ValueError("目标剧集数必须是整数") from exc
        if not 2 <= count <= 100:
            raise ValueError("目标剧集数必须在 2 至 100 集之间")
        return count

    def _requires_long_form_expansion(self, runtime: dict[str, Any] | None = None) -> bool:
        """Return whether this project requires a numbered multi-episode screenplay."""

        if runtime and "episode_count" in runtime:
            return True
        return int(getattr(self, "EXPANDED_SCRIPT_TARGET_CHARS", 0) or 0) >= self.LONG_FORM_MIN_CHARS

    @classmethod
    def _episode_count(cls, screenplay: str) -> int:
        """Count valid numbered episode headings in persisted screenplay text."""

        return len(cls._long_form_sections(screenplay))

    @classmethod
    def _next_episode_number(cls, screenplay: str) -> int:
        """Resume long-form expansion at the first missing episode number."""

        sections = cls._long_form_sections(screenplay)
        return max((section["number"] for section in sections), default=0) + 1

    @classmethod
    def _is_long_form_screenplay(
        cls, screenplay: str, runtime: dict[str, Any] | None = None
    ) -> bool:
        """Detect the requested numbered episode format without using length alone."""

        return cls._episode_count(screenplay) >= cls._target_episode_count(runtime)

    @classmethod
    def _is_resumable_long_form_checkpoint(cls, screenplay: str) -> bool:
        """Return whether a partial screenplay has safe consecutive episode boundaries."""

        sections = cls._long_form_sections(screenplay)
        numbers = [section["number"] for section in sections]
        return bool(numbers) and numbers == list(range(1, len(numbers) + 1))

    @classmethod
    def _long_form_sections(cls, screenplay: str) -> list[dict[str, Any]]:
        """Parse numbered episode blocks while retaining only unique entries."""

        # ``_clean_script`` intentionally flattens whitespace for normal short
        # screenplay prompts.  Long-form parsing instead relies on one heading
        # per line, so retain line boundaries here.
        clean = str(screenplay or "").replace("\r\n", "\n").replace("\r", "\n").strip()
        matches = list(cls._EPISODE_HEADING.finditer(clean))
        sections: list[dict[str, Any]] = []
        used_numbers: set[int] = set()
        for index, match in enumerate(matches):
            number = int(match.group(1))
            if number in used_numbers:
                continue
            body = clean[match.end(): matches[index + 1].start() if index + 1 < len(matches) else len(clean)]
            title = str(match.group(2) or f"第{number}集").strip()
            if body.strip():
                sections.append({"number": number, "name": title, "body": body.strip()})
                used_numbers.add(number)
        return sections

    def _expansion_episode_count(self, runtime: dict[str, Any] | None = None) -> int:
        """Supply compatible episode counts to existing drama skills."""

        return self._target_episode_count(runtime) if self._requires_long_form_expansion(runtime) else 12

    def _story_bible_format_requirements(self, runtime: dict[str, Any] | None = None) -> str:
        """Describe the mandatory target-episode story-bible output contract."""

        if not self._requires_long_form_expansion(runtime):
            return f"{self._creation_config_summary(runtime)}\n给出可执行的人物弧、冲突线、伏笔和结局。"
        return (
            f"{self._creation_config_summary(runtime)}\n必须规划{self._target_episode_count(runtime)}集，按连续篇章组织。逐集给出集号、集名、核心冲突、"
            "人物推进、结尾钩子和衔接状态；不要复述或模仿任何检索作品。"
        )

    def _installment_episode_card(
        self, installment: int, written_chars: int, episode_start: int | None, episode_end: int | None
    ) -> str:
        """Give the script-writing skill an unambiguous episode range."""

        if episode_start is None or episode_end is None:
            return f"第 {installment} 节，已完成约 {written_chars} 字，继续推进新的冲突。"
        return (
            f"只写第{episode_start:03d}集至第{episode_end:03d}集；每集均有独立标题、"
            "完整场景动作对白和本集结尾钩子，不能跨范围补写。"
        )

    def _installment_format_requirements(
        self,
        installment: int,
        episode_start: int | None,
        episode_end: int | None,
        target_episode_chars: int | None = None,
        installment_max_chars: int | None = None,
    ) -> str:
        """Make each expansion response resumable and easy to validate."""

        if episode_start is None or episode_end is None:
            return f"这是第 {installment} 节续写。"
        range_size = episode_end - episode_start + 1
        desired_target = max(1, int(target_episode_chars or 1_200))
        batch_limit = int(installment_max_chars or desired_target * range_size)
        maximum = max(1, batch_limit // range_size)
        target = min(desired_target, maximum)
        return (
            f"这是第 {installment} 批。只输出第{episode_start:03d}至第{episode_end:03d}集，"
            "每集以单独一行“【第001集：集名】”开始；"
            f"每集约{target}个中文字符且不超过{maximum}个中文字符，本批不超过{batch_limit}个中文字符。"
        )

    def expansion_char_limits(self, runtime: dict[str, Any] | None = None) -> tuple[int, int]:
        """Resolve one project's validated expanded-screenplay character range."""

        values = runtime or {}
        default_minimum = int(getattr(self, "EXPANDED_SCRIPT_TARGET_CHARS", 10_000) or 10_000)
        default_maximum = int(getattr(self, "EXPANDED_SCRIPT_MAX_CHARS", 50_000) or 50_000)
        try:
            minimum = int(values.get("expanded_script_min_chars", default_minimum))
            maximum = int(values.get("expanded_script_max_chars", default_maximum))
        except (TypeError, ValueError) as exc:
            raise ValueError("扩写字数范围必须是整数") from exc
        if minimum < 1 or maximum < minimum:
            raise ValueError("扩写字数最小值必须大于零且不超过最大值")
        return minimum, maximum

    def _installment_character_limit(
        self,
        expanded: str,
        maximum_chars: int,
        episode_start: int | None,
        episode_end: int | None,
        target_episode_count: int,
    ) -> int:
        """Reserve enough of the hard ceiling for every remaining episode batch."""

        if episode_start is None or episode_end is None:
            return max(1, maximum_chars - len(expanded))
        remaining_episodes = target_episode_count - episode_start + 1
        remaining_batches = max(1, math.ceil(remaining_episodes / self.LONG_FORM_EPISODES_PER_INSTALLMENT))
        joiner_length = 2 if expanded else 0
        remaining_chars = maximum_chars - len(expanded) - joiner_length
        if remaining_chars < remaining_batches * 200:
            raise RuntimeError(f"扩写字数上限 {maximum_chars} 字不足以完成剩余剧集")
        return max(200, remaining_chars // remaining_batches)

    @staticmethod
    def _streamed_expansion_preview(chunks: list[str], delta: str, maximum_chars: int) -> str:
        """Keep browser previews within the same hard ceiling as saved checkpoints."""

        prefix = "\n\n".join(chunks).strip()
        joiner = "\n\n" if prefix else ""
        available = max(0, maximum_chars - len(prefix) - len(joiner))
        return f"{prefix}{joiner}{str(delta)[:available]}".strip()

    def _fit_installment_within_limit(
        self,
        chunk: str,
        maximum_chars: int,
        episode_start: int | None,
        episode_end: int | None,
    ) -> str:
        """Compact an overlong provider installment while retaining every episode heading."""

        if len(chunk) <= maximum_chars:
            return chunk
        if episode_start is None or episode_end is None:
            return chunk[:maximum_chars].rstrip()
        sections = self._long_form_sections(chunk)
        expected_numbers = list(range(episode_start, episode_end + 1))
        if [section["number"] for section in sections] != expected_numbers:
            return chunk[:maximum_chars].rstrip()
        headings = [f"【第{section['number']:03d}集：{section['name']}】" for section in sections]
        separators = 2 * (len(sections) - 1)
        body_budget = maximum_chars - sum(map(len, headings)) - separators - len(sections)
        if body_budget < len(sections):
            raise RuntimeError(f"扩写字数上限 {maximum_chars} 字不足以保留本批剧集结构")
        base, remainder = divmod(body_budget, len(sections))
        compact = []
        for index, (heading, section) in enumerate(zip(headings, sections, strict=True)):
            limit = base + (1 if index < remainder else 0)
            compact.append(f"{heading}\n{section['body'][:limit].rstrip()}")
        return "\n\n".join(compact).strip()

    def _research_story_frameworks(
        self,
        agent: Any,
        source_excerpt: str,
        runtime: dict[str, Any],
        on_stage: Callable[[str], None] | None = None,
        is_cancelled: Callable[[], bool] | None = None,
    ) -> str:
        """Run four web-search-assisted framework studies before expansion.

        This is called only for the long-drama creation flow.  It asks for
        transferable structure rather than titles, characters, or copyrighted
        plot text, then supplies the compact notes to the story-bible request.
        """

        notes: list[str] = []
        empty_topics: list[str] = []
        for index, focus in enumerate(self.LONG_FORM_RESEARCH_TOPICS, start=1):
            self._raise_if_expansion_cancelled(is_cancelled)
            if on_stage:
                on_stage(
                    f"正在联网研究同类故事框架（{index}/{len(self.LONG_FORM_RESEARCH_TOPICS)}）：{focus}"
                )
            try:
                skill = agent.execute_skill(
                    "story_framework_researcher",
                    {"premise": source_excerpt, "topic": focus},
                )
            except Exception as exc:
                raise RuntimeError(f"联网叙事框架研究准备失败：{focus}：{exc}") from exc
            response = self._stream_completion_with_retry(
                agent,
                f"联网叙事框架研究：{focus}",
                [{
                    "role": "user",
                    "content": (
                        f"研究技能：{skill.get('instruction', '')}\n"
                        "请使用 web_search 查询与下列创意在类型、受众或叙事节奏上相近的"
                        "公开小说、短剧或影视作品介绍；四轮合计覆盖3至4个不同作品。只总结可迁移的"
                        "抽象叙事框架。不要复述原文、不要输出作品人物名、专有剧情或长引用。重点："
                        + focus + "。\n"
                        "用户创意：\n" + source_excerpt
                    ),
                }],
                runtime,
                lambda _delta: None,
                tools=[{"type": "web_search"}],
                is_cancelled=is_cancelled,
            )
            self._raise_if_expansion_cancelled(is_cancelled)
            clean = self._clean_expansion_text(response)
            if clean:
                notes.append(f"【{focus}】{clean[:2_000]}")
            else:
                empty_topics.append(focus)
        if len(notes) < 3:
            detail = f"；无有效返回：{'、'.join(empty_topics)}" if empty_topics else ""
            raise RuntimeError(
                f"联网同类故事框架研究不足 3 条（当前 {len(notes)} 条）{detail}，无法开始{self._target_episode_count(runtime)}集剧本扩写"
            )
        return "\n\n".join(notes)

    def _plan_long_form(
        self,
        screenplay: str,
        runtime: dict[str, Any],
        agent: Any,
        is_cancelled: Callable[[], bool] | None = None,
    ) -> dict[str, Any]:
        """Generate a storyboard skeleton in bounded episode batches."""

        self._raise_if_expansion_cancelled(is_cancelled)
        target_episode_count = self._target_episode_count(runtime)
        sections = self._long_form_sections(screenplay)
        if len(sections) < target_episode_count:
            raise ValueError(f"扩写剧本不足{target_episode_count}集，不能进入长剧分镜")
        sections = sections[:target_episode_count]
        assets: list[dict[str, Any]] = []
        episodes: list[dict[str, Any]] = []
        for start in range(0, len(sections), self.LONG_FORM_BATCH_SIZE):
            self._raise_if_expansion_cancelled(is_cancelled)
            batch = sections[start:start + self.LONG_FORM_BATCH_SIZE]
            stage = f"第{batch[0]['number']}至{batch[-1]['number']}集分镜骨架"
            messages = [{"role": "user", "content": self._long_form_batch_prompt(batch, runtime)}]
            response = (
                self._stream_completion_with_retry(
                    agent, stage, messages, runtime, lambda _delta: None,
                    tools=[{"type": "web_search"}], is_cancelled=is_cancelled,
                )
                if is_cancelled
                else self._completion_with_retry(
                    agent, stage, messages, runtime, tools=[{"type": "web_search"}],
                )
            )
            self._raise_if_expansion_cancelled(is_cancelled)
            parsed = self._parse_json(response)
            batch_episodes, batch_assets = self._long_form_batch_result(parsed, batch, runtime)
            episodes.extend(batch_episodes)
            assets.extend(batch_assets)
        normalized = self._normalize_plan({"episodes": episodes, "assets": assets}, screenplay, runtime)
        if len(normalized["episodes"]) < target_episode_count:
            raise RuntimeError(f"长剧分镜结果不足{target_episode_count}集，请重新执行分镜")
        return {"episodes": normalized["episodes"][:target_episode_count], "assets": normalized["assets"]}

    def _fallback_long_form_plan(self, screenplay: str, runtime: dict[str, Any]) -> dict[str, Any]:
        """Keep an editable target-episode skeleton available without a provider."""

        sections = self._long_form_sections(screenplay)[:self._target_episode_count(runtime)]
        episodes = [{"name": f"第{item['number']}集：{item['name']}", "shots": self._fallback_long_form_shots(item, runtime)} for item in sections]
        return {"episodes": episodes, "assets": self._fallback_asset_catalog(screenplay, runtime)}

    def _long_form_batch_prompt(self, batch: list[dict[str, Any]], runtime: dict[str, Any]) -> str:
        """Create the provider prompt for one range of expanded episodes."""

        constraints = runtime.get("shot_constraints") or {}
        subtitle_rule = "不得出现字幕、屏幕文字或水印。" if not constraints.get("subtitles", False) else "按剧情需要标注字幕信息。"
        music_rule = "不得出现背景音乐描述。" if not constraints.get("background_music", False) else "可按剧情需要标注背景音乐。"
        source = "\n\n".join(
            f"【第{item['number']:03d}集：{item['name']}】\n{item['body']}" for item in batch
        )
        return (
            "你正在生成长剧的分镜骨架。请使用 web_search 仅在需要校验通用类型节奏时查询，"
            "不得复制任何搜索到的作品内容。只返回合法 JSON，格式："
            '{"episodes":[{"name":"第001集：集名","shots":[{"title":"","original_text":"","prompt":"","duration":10}]}],'
            '"assets":[{"id":"","type":"character|scene|prop","name":"","prompt":""}]}。\n'
            f"必须且只能拆解第{batch[0]['number']:03d}至第{batch[-1]['number']:03d}集，不得合并、遗漏或新增集数。"
            f"每集生成2至4条按顺序衔接的分镜；每条分镜默认 duration 为 10 秒；original_text 不超过{self._shot_script_char_limit(runtime)}字，只能来自该集的局部事件，不能复制整集或整剧。"
            "每个 prompt 必须含有‘场景：’、‘角色：’、‘风格：’、‘光线：’、‘位置：’以及至少两个‘【镜头’段落，"
            "并使用 @图1（场景）、@图2（角色）、@图3（道具）形式预留参考图，文本需分段换行。"
            f"{self._creation_config_summary(runtime)}{subtitle_rule}{music_rule}\n"
            "素材只产出故事中真实有意义、可复用的角色/场景/道具，角色使用人名而非身份代称。\n"
            "本批剧本：\n" + source
        )

    def _long_form_batch_result(
        self, raw: dict[str, Any], batch: list[dict[str, Any]], runtime: dict[str, Any]
    ) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
        """Pair imperfect provider JSON with immutable expanded episode content."""

        raw_episodes = raw.get("episodes") if isinstance(raw, dict) else []
        raw_episodes = raw_episodes if isinstance(raw_episodes, list) else []
        by_number = {self._episode_number_from_name(item.get("name") or item.get("title")): item for item in raw_episodes if isinstance(item, dict)}
        episodes: list[dict[str, Any]] = []
        for index, section in enumerate(batch):
            candidate = by_number.get(section["number"]) or (raw_episodes[index] if index < len(raw_episodes) and isinstance(raw_episodes[index], dict) else {})
            raw_shots = candidate.get("shots") or candidate.get("storyboards") or []
            shots = self._long_form_shots_from_response(raw_shots, section, runtime)
            episodes.append({"name": f"第{section['number']}集：{section['name']}", "shots": shots})
        assets = raw.get("assets") if isinstance(raw, dict) and isinstance(raw.get("assets"), list) else []
        return episodes, [item for item in assets if isinstance(item, dict)]

    def _long_form_shots_from_response(
        self, raw_shots: Any, section: dict[str, Any], runtime: dict[str, Any]
    ) -> list[dict[str, Any]]:
        """Validate each response shot and fall back to unique script segments."""

        fallback = self._fallback_long_form_shots(section, runtime)
        if not isinstance(raw_shots, list) or not 2 <= len(raw_shots) <= 4:
            return fallback
        segments = self._split_script_into_segments(section["body"], len(raw_shots))
        shots: list[dict[str, Any]] = []
        for index, raw in enumerate(raw_shots):
            if not isinstance(raw, dict):
                return fallback
            prompt = str(raw.get("prompt") or "").strip()
            if not self._is_rich_long_form_prompt(prompt):
                prompt = fallback[index]["prompt"]
            shots.append({
                "id": str(raw.get("id") or f"shot_{section['number']}_{index + 1}"),
                "title": str(raw.get("title") or f"第{section['number']}集镜头{index + 1}"),
                "original_text": segments[index] if index < len(segments) else section["body"][:160],
                "prompt": prompt,
                "duration": min(15, max(3, int(raw.get("duration") or 10))),
            })
        return shots

    def _fallback_long_form_shots(self, section: dict[str, Any], runtime: dict[str, Any]) -> list[dict[str, Any]]:
        """Create three distinct editable shots when a batch response is weak."""

        segments = self._split_script_into_segments(section["body"], 3)
        return [{
            "id": f"shot_{section['number']}_{index + 1}",
            "title": f"第{section['number']}集镜头{index + 1}",
            "original_text": segment,
            "prompt": self._fallback_long_form_prompt(section, segment, index + 1, runtime),
            "duration": 10,
        } for index, segment in enumerate(segments)]

    @staticmethod
    def _episode_number_from_name(value: Any) -> int | None:
        """Extract an episode number from tolerant model labels."""

        match = re.search(r"第\s*(\d{1,3})\s*集", str(value or ""))
        return int(match.group(1)) if match else None

    @staticmethod
    def _is_rich_long_form_prompt(prompt: str) -> bool:
        """Require the rich text shape used by the video prompt editor."""

        return all(token in prompt for token in ("场景：", "角色：", "风格：", "光线：", "位置：", "【镜头", "@图"))

    @staticmethod
    def _fallback_long_form_prompt(section: dict[str, Any], segment: str, index: int, runtime: dict[str, Any]) -> str:
        """Return an editable rich-text prompt with default future references."""

        style = str(runtime.get("style") or "真人风格")
        theme = str(runtime.get("theme") or "都市")
        return (
            "场景：\n@图1（待匹配场景）\n\n角色：\n@图2（待匹配角色）\n\n"
            "道具：\n@图3（待匹配道具）\n\n"
            f"风格：{style}，叙述背景主题为{theme}，细节丰富。\n"
            "光线：根据当前剧情情绪设置自然且连续的主光。\n"
            "位置：@图2 位于画面主体区域，与 @图1 的空间关系清晰。\n\n"
            f"【镜头{index} | 时长5s | 时间：按剧情】中景，平视，镜头稳定推进。@图2 {segment}\n"
            f"【镜头{index + 1} | 时长5s | 时间：按剧情】近景，轻微推近，承接前一动作并留下下一镜线索。"
        )
