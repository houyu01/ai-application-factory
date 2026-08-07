"""Research skill instructions for the long-form drama expansion flow."""

from src.llm_service.skills.base import BaseSkill


class StoryFrameworkResearcherSkill(BaseSkill):
    """Prepare safe web-research instructions before a long drama is expanded.

    ``ScriptPlannerLongFormMixin`` invokes this skill before it writes a
    target-episode story bible.  It keeps web findings at the abstract framework
    level so the generated screenplay learns pacing and structure without
    copying plots, prose, characters, or protected story worlds.
    """

    name = "story_framework_researcher"
    description = "研究同类型小说的抽象剧情框架，为多集短剧扩写提供节奏参考。"
    parameters = {
        "type": "object",
        "properties": {
            "premise": {"type": "string", "description": "待扩写的故事梗概"},
            "topic": {"type": "string", "description": "本轮研究的结构主题"},
        },
        "required": ["premise", "topic"],
        "additionalProperties": False,
    }
    instruction = """
使用 web_search 查询 3 至 4 个与给定题材相近的公开小说、短剧或影视作品介绍，
只归纳其可迁移的结构规律：开篇钩子、人物关系推进、中段升级、反转节奏、伏笔回收与结局。
不得复述原作品剧情、角色名、段落、对白或世界观专有名词；输出的每条规律都必须能改写为原创故事。
""".strip()
