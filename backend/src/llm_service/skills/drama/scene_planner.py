from src.llm_service.skills.base import BaseSkill


class ScenePlannerSkill(BaseSkill):
    """Skill that identifies and structures reusable drama locations."""
    name = "scene_planner"
    description = "把单集剧情卡拆成有因果关系的分场和镜头目标。"
    parameters = {
        "type": "object",
        "properties": {
            "episode_card": {"type": "string"},
            "story_bible": {"type": "string"},
            "previous_episode_summary": {"type": "string"},
            "scene_count": {"type": "integer"},
        },
        "required": [
            "episode_card",
            "story_bible",
            "previous_episode_summary",
            "scene_count",
        ],
        "additionalProperties": False,
    }
    instruction = """
将单集拆分为指定数量的场景。每个场景输出 location、time、characters、purpose、
conflict、action、dialogue_goal、visual_goal、transition 和 cliffhanger。场景之间
要明确承接，并服务于本集的 turning_point 和 ending_hook。
""".strip()
