from typing import Any

from src.llm_service.skills.base import BaseSkill, SkillContext


class StoryBibleGeneratorSkill(BaseSkill):
    """Skill that extracts a stable story bible from a drama premise."""
    name = "story_bible_generator"
    description = "根据创意和扩写结果生成可长期维护的故事圣经。"
    parameters = {
        "type": "object",
        "properties": {
            "premise": {"type": "string"},
            "expanded_concept": {"type": "string"},
            "episode_count": {"type": "integer"},
            "format_requirements": {"type": "string"},
        },
        "required": [
            "premise",
            "expanded_concept",
            "episode_count",
            "format_requirements",
        ],
        "additionalProperties": False,
    }
    instruction = """
建立故事圣经，至少包含：角色档案、人物关系、角色成长弧、世界规则、时间线、
主线和支线、伏笔清单、悬念清单、重要道具、场景库、内容边界和最终结局。
所有事实要稳定、可检索、可用于后续{episode_count}集的创作。
{format_requirements}
""".strip()

    def execute(self, arguments: dict[str, Any], context: SkillContext) -> dict[str, Any]:
        """Include the project's episode contract in the planning instruction."""

        result = super().execute(arguments, context)
        result["instruction"] = self.instruction.format(
            episode_count=int(arguments["episode_count"]),
            format_requirements=str(arguments["format_requirements"]),
        )
        return result
