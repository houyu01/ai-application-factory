from src.llm_service.skills.base import BaseSkill


class ShotPromptGeneratorSkill(BaseSkill):
    name = "shot_prompt_generator"
    description = "将分镜文本和已保存的角色、场景、道具素材融合成视频生成 Prompt。"
    parameters = {
        "type": "object",
        "properties": {
            "project_name": {"type": "string"},
            "shot_title": {"type": "string"},
            "storyboard_text": {"type": "string"},
            "assets": {"type": "string"},
            "style": {"type": "string"},
            "ratio": {"type": "string"},
            "duration": {"type": "integer"},
        },
        "required": [
            "project_name",
            "shot_title",
            "storyboard_text",
            "assets",
            "style",
            "ratio",
            "duration",
        ],
        "additionalProperties": False,
    }
    instruction = """
输出一段自然的中文视频生成提示词，不要输出 JSON 或解释。按“场景、角色、道具、动作、
镜头、光线、风格、时长”组织，但不要生硬拼接字段。必须优先使用已保存素材的描述，
不要臆造不存在的人物、地点或道具；要交代镜头起始状态和结束状态，保证相邻分镜可衔接。
""".strip()
