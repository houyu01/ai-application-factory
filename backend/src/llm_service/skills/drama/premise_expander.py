from typing import Any

from src.llm_service.skills.base import BaseSkill, SkillContext


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
            "target_min_chars": {"type": "integer", "description": "扩写剧本最小字数"},
            "target_max_chars": {"type": "integer", "description": "扩写剧本最大字数"},
        },
        "required": [
            "premise", "genre", "target_audience", "episode_count",
            "target_min_chars", "target_max_chars",
        ],
        "additionalProperties": False,
    }
    instruction = """
扩展一句话创意，为{episode_count}集、至少{target_min_chars}字、最多{target_max_chars}字的完整剧本设计足够的剧情容量，输出结构化的故事核心设定：logline、核心冲突、主题、
世界观、主要人物关系、主线悬念、阶段性目标和结局方向。请尽量参考市面上的常见对应类型的小说(使用目前的故事类型，去互联网上参考类似的小说，进行剧情扩写)
扩展的内容包括但不限于：
1. 故事的反转(如好人坏人之间的关系反转)
2. 故事的递进(从最开始故事简单的上下文，扩展出更多故事的内容和信息)
3. 人物角色的适当补充(在初始或中间剧集中，增加结局需要的部分关键人物)
4. 分支剧情(对结局影响不大，但是过程中主角遇到的一些分支任务或剧情，最好也可以为结局埋下一定伏笔)
5. 平缓的内容与激烈进展的内容(如果是古装剧，则可以扩展文戏和打戏，如果是现代剧，则可以一些心灵鸡汤和突然几集的关键冲突)
6. 可以适当增加主角或其他角色的成长历程描写
7. 如果是群像剧的话，可以适当花一部分篇幅扩展配角的成长历程，但是不宜过多
""".strip()

    def execute(self, arguments: dict[str, Any], context: SkillContext) -> dict[str, Any]:
        """Apply the project-level screenplay range to this LLM instruction."""

        result = super().execute(arguments, context)
        minimum = int(arguments["target_min_chars"])
        maximum = int(arguments["target_max_chars"])
        result["instruction"] = self.instruction.format(
            episode_count=int(arguments["episode_count"]),
            target_min_chars=f"{minimum:,}", target_max_chars=f"{maximum:,}"
        )
        return result
