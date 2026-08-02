from src.llm_service.skills.base import BaseSkill


class ScriptWriterSkill(BaseSkill):
    name = "script_writer"
    description = "根据分场计划生成单集剧本、动作、对白和可视化信息。"
    parameters = {
        "type": "object",
        "properties": {
            "story_bible": {"type": "string"},
            "episode_card": {"type": "string"},
            "scene_plan": {"type": "string"},
            "style_requirements": {"type": "string"},
        },
        "required": [
            "story_bible",
            "episode_card",
            "scene_plan",
            "style_requirements",
        ],
        "additionalProperties": False,
    }
    instruction = """
严格按照 scene_plan 逐场生成剧本。每场包含场景标题、人物动作、对白、情绪、
镜头/画面提示和声音提示。保持人物口吻、世界规则和时间线一致，不擅自解决
未到回收时机的伏笔，并在结尾保留本集的追更钩子。
""".strip()
