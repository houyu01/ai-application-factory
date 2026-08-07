"""Regression coverage for the 50-episode, web-researched drama flow."""

from __future__ import annotations

import json
import re

import pytest

from src.llm_service.planner import ScriptPlanner
from src.llm_service.skills import SkillContext
from src.llm_service.skills.drama.premise_expander import PremiseExpanderSkill


class RecordingLongFormAgent:
    """Deterministic language agent for long-form planning contract tests.

    The test uses this agent to verify the planner's orchestration boundary:
    each provider call records whether web search was requested, while generated
    installments and storyboard batches remain compact enough for unit tests.
    """

    def __init__(self) -> None:
        self.completions: list[tuple[str, dict]] = []
        self.skill_calls: list[tuple[str, dict]] = []

    def execute_skill(self, name: str, arguments: dict) -> dict:
        """Record the requested skill and return a deterministic instruction."""

        self.skill_calls.append((name, arguments))
        return {"instruction": f"{name} instruction"}

    def completion(self, messages: list[dict], **kwargs) -> str:
        """Return research, installments, or JSON storyboards by stage."""

        prompt = str(messages[-1]["content"])
        self.completions.append((prompt, kwargs))
        if "只返回合法 JSON" in prompt:
            return self._storyboard_batch(prompt)
        installment = re.search(r"只输出第(\d{3})至第(\d{3})集", prompt)
        if installment:
            return self._installment(int(installment.group(1)), int(installment.group(2)))
        return "原创长剧结构笔记：开篇钩子、关系升级、中段反转、伏笔回收和终局抉择。"

    async def completion_stream(self, messages: list[dict], **kwargs):
        """Expose the streaming contract used while long-form installments are written."""

        yield self.completion(messages, **kwargs)

    @staticmethod
    def _installment(start: int, end: int) -> str:
        """Produce five numbered episodes with enough text to meet the target."""

        episodes: list[str] = []
        for number in range(start, end + 1):
            paragraph = (
                f"林岩在第{number}集沿旧城线索推进调查，道玄提出新的条件，两人在误会中被迫合作。"
                "他们进入与主题相符的场景，发现一件能改变人物关系的关键证据；冲突升级后，"
                "主角做出有代价的选择，并在结尾留下下一集必须解决的悬念。"
            )
            episodes.append(f"【第{number:03d}集：线索反转{number}】\n" + paragraph * 35)
        return "\n\n".join(episodes)

    @staticmethod
    def _storyboard_batch(prompt: str) -> str:
        """Return a rich-text storyboard payload for the requested ten episodes."""

        episode_range = re.search(r"只能拆解第(\d{3})至第(\d{3})集", prompt)
        assert episode_range is not None
        start, end = (int(episode_range.group(1)), int(episode_range.group(2)))
        episodes = []
        for number in range(start, end + 1):
            rich_prompt = (
                "场景：\n@图1（旧城线索现场）\n\n角色：\n@图2（林岩）\n\n"
                "道具：\n@图3（关键证据）\n\n风格：真人风格，叙述背景主题为都市。\n"
                "光线：日间自然光。\n位置：@图2 位于 @图1 中央并手持 @图3。\n\n"
                "【镜头1 | 时长5s】林岩检查证据。\n"
                "【镜头2 | 时长5s】道玄逼近，新的冲突出现。"
            )
            episodes.append(
                {
                    "name": f"第{number}集：线索反转{number}",
                    "shots": [
                        {"title": "发现线索", "original_text": "错误的整集文本", "prompt": rich_prompt, "duration": 10},
                        {"title": "冲突升级", "original_text": "错误的整集文本", "prompt": rich_prompt},
                    ],
                }
            )
        return json.dumps({"episodes": episodes, "assets": []}, ensure_ascii=False)


def test_short_premise_becomes_web_researched_twenty_five_episode_storyboards(monkeypatch) -> None:
    """Configured long-drama creation must research, expand, and batch-plan 25 episodes."""

    planner = ScriptPlanner()
    agent = RecordingLongFormAgent()
    monkeypatch.setattr(planner, "_agent", lambda *_args: agent)

    expanded = planner.expand_script(
        "失意青年林岩回到故乡，在旧宅发现一把会指向真相的钥匙。",
        {
            "model": "test-long-form",
            "theme": "都市悬疑",
            "style": "真人风格",
            "enable_web_search": True,
        },
    )

    assert expanded is not None
    assert len(expanded) >= 5_000
    assert planner._episode_count(expanded) >= 25
    assert [name for name, _arguments in agent.skill_calls].count("story_framework_researcher") == 4

    plan = planner.plan(expanded, {"model": "test-long-form", "theme": "都市悬疑"})

    assert len(plan["episodes"]) == 25
    assert all(2 <= len(episode["shots"]) <= 4 for episode in plan["episodes"])
    assert all(
        shot["original_text"] != "错误的整集文本"
        and "场景：" in shot["prompt"]
        and "角色：" in shot["prompt"]
        and "【镜头" in shot["prompt"]
        and shot["duration"] == 10
        for episode in plan["episodes"]
        for shot in episode["shots"]
    )
    assert all(kwargs.get("tools") == [{"type": "web_search"}] for _prompt, kwargs in agent.completions)


def test_requested_episode_count_controls_storyboard_count(monkeypatch) -> None:
    """A project target must replace the former fixed fifty-episode contract."""

    planner = ScriptPlanner()
    agent = RecordingLongFormAgent()
    screenplay = RecordingLongFormAgent._installment(1, 7)
    monkeypatch.setattr(planner, "_agent", lambda *_args: agent)

    plan = planner.plan(screenplay, {"model": "test-long-form", "episode_count": 7})

    assert len(plan["episodes"]) == 7
    assert "只能拆解第001至第007集" in agent.completions[0][0]
    assert "每个分镜剧本文字不超过400字" in agent.completions[0][0]
    assert all(len(shot["original_text"]) <= 400 for episode in plan["episodes"] for shot in episode["shots"])


def test_long_form_storyboards_accept_the_worker_cancellation_callback(monkeypatch) -> None:
    """The durable decomposition worker can use the cancellable stream path."""

    planner = ScriptPlanner()
    agent = RecordingLongFormAgent()
    screenplay = RecordingLongFormAgent._installment(1, 7)
    monkeypatch.setattr(planner, "_agent", lambda *_args: agent)

    plan = planner.plan(
        screenplay,
        {"model": "test-long-form", "episode_count": 7},
        is_cancelled=lambda: False,
    )

    assert len(plan["episodes"]) == 7
    assert agent.completions


def test_long_form_expansion_stays_within_the_project_character_range(monkeypatch) -> None:
    """Provider installments must be compacted without dropping the fifty-episode contract."""

    planner = ScriptPlanner()
    agent = RecordingLongFormAgent()
    previews: list[str] = []
    monkeypatch.setattr(planner, "_agent", lambda *_args: agent)

    expanded = planner.expand_script(
        "林岩回到故乡，在旧宅发现一把钥匙。",
        {
            "model": "test-long-form",
            "episode_count": 50,
            "expanded_script_min_chars": 50_000,
            "expanded_script_max_chars": 60_000,
        },
        stream=previews.append,
    )

    assert expanded is not None
    assert 50_000 <= len(expanded) <= 60_000
    assert planner._episode_count(expanded) == 50
    assert previews and max(map(len, previews)) <= 60_000
    prompts = [prompt for prompt, _kwargs in agent.completions]
    assert any("本批不超过6000个中文字符" in prompt for prompt in prompts)


def test_story_bible_prompt_uses_the_project_character_range() -> None:
    """The creation form's range must reach both the skill and the LLM prompt."""

    planner = ScriptPlanner()
    agent = RecordingLongFormAgent()
    runtime = {
        "episode_count": 8,
        "expanded_script_min_chars": 60_000,
        "expanded_script_max_chars": 80_000,
        "shot_script_max_chars": 240,
    }

    planner._build_expansion_outline(agent, "林岩在旧宅寻找钥匙。", runtime)

    premise_arguments = next(
        arguments for name, arguments in agent.skill_calls if name == "premise_expander"
    )
    assert premise_arguments["target_min_chars"] == 60_000
    assert premise_arguments["target_max_chars"] == 80_000
    assert premise_arguments["episode_count"] == 8
    assert premise_arguments["shot_script_max_chars"] == 240
    assert "目标剧集数=8集" in agent.completions[-1][0]
    assert "必须规划8集" in planner._story_bible_format_requirements(runtime)
    assert "扩写剧本总字数=60000至80000字" in agent.completions[-1][0]
    assert "每个分镜剧本文字不超过240字" in agent.completions[-1][0]

    instruction = PremiseExpanderSkill().execute(
        {
            "premise": "林岩在旧宅寻找钥匙。",
            "genre": "悬疑",
            "target_audience": "短剧观众",
            "episode_count": 50,
            "target_min_chars": 60_000,
            "target_max_chars": 80_000,
            "shot_script_max_chars": 240,
        },
        SkillContext(agent_name="test"),
    )["instruction"]
    assert "至少60,000字、最多80,000字" in instruction
    assert "每个分镜剧本文字不超过240字" in instruction


def test_long_form_creation_requires_a_configured_language_model(monkeypatch) -> None:
    """Avoid silently falling back to a short local screenplay for a long drama."""

    planner = ScriptPlanner()
    monkeypatch.setattr(planner, "_agent", lambda *_args: None)

    with pytest.raises(RuntimeError, match="未配置可调用的语言模型"):
        planner.expand_script("一个短剧创意。")


def test_partial_numbered_screenplay_is_a_resumable_checkpoint() -> None:
    """A completed first five-episode batch must survive the next worker retry."""

    planner = ScriptPlanner()
    checkpoint = "\n\n".join(
        f"【第{number:03d}集：续写测试】\n林岩在本集推进调查，并在结尾留下新悬念。"
        for number in range(1, 6)
    )

    assert planner._is_resumable_long_form_checkpoint(checkpoint) is True
    assert planner._is_resumable_long_form_checkpoint("没有集号的旧版文本") is False


def test_retry_reuses_story_bible_and_continues_after_saved_episodes(monkeypatch) -> None:
    """A retry must retain the outline and continue from the next durable episode."""

    planner = ScriptPlanner()
    agent = RecordingLongFormAgent()
    existing = RecordingLongFormAgent._installment(1, 5)
    monkeypatch.setattr(planner, "_agent", lambda *_args: agent)

    expanded = planner.expand_script(
        "林岩回到故乡，在旧宅发现一把钥匙。",
        {"model": "test-long-form", "expanded_script_max_chars": 60_000},
        existing_script=existing,
        existing_outline="已保存的故事圣经：人物弧、冲突线和终局回收。",
    )

    assert expanded is not None and expanded.startswith(existing)
    prompts = [prompt for prompt, _kwargs in agent.completions]
    assert all("建立紧凑故事圣经" not in prompt for prompt in prompts)
    assert any("只输出第006至第010集" in prompt for prompt in prompts)
