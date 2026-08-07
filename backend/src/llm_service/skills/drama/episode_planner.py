from src.llm_service.skills.base import BaseSkill


class EpisodePlannerSkill(BaseSkill):
    """Skill that groups generated shots into coherent episodes."""
    name = "episode_planner"
    description = "把故事圣经拆解为连续、可追更的多集剧情卡。"
    parameters = {
        "type": "object",
        "properties": {
            "story_bible": {"type": "string"},
            "episode_start": {"type": "integer"},
            "episode_end": {"type": "integer"},
            "previous_state": {"type": "string"},
        },
        "required": [
            "story_bible",
            "episode_start",
            "episode_end",
            "previous_state",
        ],
        "additionalProperties": False,
    }
    instruction = """
为指定集数生成剧情卡。每集必须有 episode_title、dramatic_goal、main_conflict、
turning_point、ending_hook、character_changes、introduced_clues、resolved_clues、
next_episode_question。确保事件有因果关系，避免连续多集重复同一种冲突。
长剧模式必须严格保留输入的集号范围：按10个五集篇章推进，任何一集都不可合并、
跳过或改号。每集都要有独立目标、可视化冲突、人物状态变化、下一集钩子，并同时
持续推进主线、人物关系线、对手线或伏笔线中的至少两条。
""".strip()
