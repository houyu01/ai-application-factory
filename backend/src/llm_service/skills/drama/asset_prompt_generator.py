from src.llm_service.skills.base import BaseSkill


class AssetPromptGeneratorSkill(BaseSkill):
    name = "asset_prompt_generator"
    description = "为短剧中的角色、场景和道具编写可复用、可生成图片的视觉提示词。"
    parameters = {
        "type": "object",
        "properties": {
            "asset_type": {"type": "string", "enum": ["character", "scene", "prop"]},
            "name": {"type": "string"},
            "story_context": {"type": "string"},
            "style": {"type": "string"},
        },
        "required": ["asset_type", "name", "story_context", "style"],
        "additionalProperties": False,
    }
    instruction = """
角色提示词需要包含年龄、外观、服装、发型、气质和可持续复用的固定特征；场景提示词
需要包含空间结构、时间、天气、材质、光线和镜头可用的环境细节；道具提示词需要包含
形状、材质、颜色、磨损和故事中的关键识别特征。只描述视觉事实，不输出图片 URL。
""".strip()
