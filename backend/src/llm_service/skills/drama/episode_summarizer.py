from src.llm_service.skills.base import BaseSkill


class EpisodeSummarizerSkill(BaseSkill):
    """Skill that summarizes episode-level narrative context."""
    name = "episode_summarizer"
    description = "把已完成的一集压缩为下一集可使用的事实和状态。"
    parameters = {
        "type": "object",
        "properties": {
            "episode_number": {"type": "integer"},
            "episode_script": {"type": "string"},
            "previous_state": {"type": "string"},
        },
        "required": ["episode_number", "episode_script", "previous_state"],
        "additionalProperties": False,
    }
    instruction = """
生成下一集使用的状态摘要，包含 plot_summary、confirmed_facts、character_states、
relationship_changes、new_clues、resolved_clues、open_questions、location_state、
prop_state 和 next_episode_hooks。只记录剧本明确发生的事实，不补写推测。
""".strip()
