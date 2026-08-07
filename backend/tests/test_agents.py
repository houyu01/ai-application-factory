from __future__ import annotations

import asyncio

from src.llm_service.agents.base_agent import BaseAgent
from src.llm_service.agents.drama_agent import DramaAgent
from src.llm_service.agents.interactive_game_agent import InteractiveGameAgent
from src.llm_service.skills.loader import SkillLoader


class FakeLLMClient:
    def __init__(self):
        self.completion_request = None
        self.stream_request = None

    def completion(self, messages, **kwargs):
        self.completion_request = (messages, kwargs)
        return "done"

    async def completion_stream(self, messages, **kwargs):
        self.stream_request = (messages, kwargs)
        yield "ok"


def test_default_loader_and_drama_agent_find_all_drama_skills():
    names = set(SkillLoader().load())
    drama_agent = DramaAgent(llm_client=FakeLLMClient())
    game_agent = InteractiveGameAgent(llm_client=FakeLLMClient())

    drama_names = {
        "premise_expander",
        "story_bible_generator",
        "episode_planner",
        "scene_planner",
            "script_writer",
            "continuity_checker",
            "episode_summarizer",
            "script_decomposer",
            "asset_prompt_generator",
            "shot_prompt_generator",
        }
    assert drama_names.issubset(names)
    assert "interactive_branch_planner" in names
    assert set(drama_agent.skills) == drama_names
    assert set(game_agent.skills) == {"interactive_branch_planner"}


def test_base_agent_completion_exposes_loaded_skills_as_tools():
    llm = FakeLLMClient()
    agent = DramaAgent(llm_client=llm)

    result = agent.completion([{"role": "user", "content": "扩写这个创意"}])

    assert result == "done"
    messages, request = llm.completion_request
    assert messages[0]["role"] == "system"
    assert request["tools"] == agent.skill_tools
    assert request["tool_executor"] == agent._execute_skill


def test_base_agent_completion_merges_provider_tools_with_skills():
    llm = FakeLLMClient()
    agent = DramaAgent(llm_client=llm)

    agent.completion(
        [{"role": "user", "content": "扩写并拆分剧本"}],
        tools=[{"type": "web_search"}],
    )

    _, request = llm.completion_request
    assert request["tools"] == [*agent.skill_tools, {"type": "web_search"}]


def test_base_agent_stream_delegates_to_llm_client():
    llm = FakeLLMClient()
    agent = DramaAgent(llm_client=llm)

    chunks = asyncio.run(_collect(agent))

    assert chunks == ["ok"]
    _, request = llm.stream_request
    assert request["tools"] == agent.skill_tools


async def _collect(agent: BaseAgent) -> list[str]:
    return [chunk async for chunk in agent.completion_stream([])]
