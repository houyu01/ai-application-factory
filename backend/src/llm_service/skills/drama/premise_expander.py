from src.llm_service.skills.base import BaseSkill


class PremiseExpanderSkill(BaseSkill):
    """Skill that expands a short premise into usable drama material."""
    name = "premise_expander"
    description = "将一句话创意扩展为可持续创作的故事核心设定。"
    parameters = {
        "type": "object",
        "properties": {
            "premise": {"type": "string", "description": "一句话故事创意"},
            "genre": {"type": "string", "description": "故事类型"},
            "target_audience": {"type": "string", "description": "目标受众"},
            "episode_count": {"type": "integer", "description": "计划集数"},
        },
        "required": ["premise", "genre", "target_audience", "episode_count"],
        "additionalProperties": False,
    }
    instruction = """
扩展一句话创意，尽量扩展到1000字以上，输出结构化的故事核心设定：logline、核心冲突、主题、
世界观、主要人物关系、主线悬念、阶段性目标和结局方向。
""".strip()
