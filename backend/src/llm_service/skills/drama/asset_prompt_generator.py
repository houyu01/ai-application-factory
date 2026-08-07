from src.llm_service.skills.base import BaseSkill


class AssetPromptGeneratorSkill(BaseSkill):
    """Skill that writes character, scene, and prop image prompts."""
    name = "asset_prompt_generator"
    description = "为短剧中的角色、场景和道具编写可复用、可生成图片的视觉提示词。"
    parameters = {
        "type": "object",
        "properties": {
            "asset_type": {"type": "string", "enum": ["character", "scene", "prop"]},
            "name": {"type": "string"},
            "story_context": {"type": "string"},
            "style": {"type": "string"},
            "theme": {"type": "string", "description": "短剧的叙述背景主题及时代世界观"},
        },
        "required": ["asset_type", "name", "story_context", "style", "theme"],
        "additionalProperties": False,
    }
    instruction = """
这是独立的视觉素材设定，不要复制完整故事原文。角色 name 必须是简短、真实、方便观众记忆的人名，
优先使用 2～4 个字，例如“林岩”“道玄”“苏晚”；不要把身份、职业和叙事功能写进 name，
也不要使用“山村遗孤·少年剑修”这类代称。角色提示词分两段：第一段写身世、性格、年龄或角色功能。性格不能只写“坚毅”“温柔”等单个形容词，
必须补充至少 3 个可观察的行为特征，例如待人方式、压力下的情绪反应、思考或决策习惯、对重要人物的态度、说话或行动倾向；
第二段写年龄/性别、脸型、肤色、
眉眼、发型、身型、服装和衣料等稳定视觉特征。场景名称必须有真实地点含义，提示词按场景由来、
外形与色调、场景中物品状态、整体氛围、人物与背景文字限制组织。道具名称必须有真实叙事含义，
提示词写颜色、材质、形状、纹理、细节、磨损、装饰和表面文字。只描述视觉事实，不输出图片 URL，
不要使用“主要角色”“主要场景”“关键道具”等泛化名称。每个素材 prompt 第一行必须原样写入
“叙述背景主题：{theme}”，并让角色服饰与身份、场景建筑与陈设、道具造型与工艺符合该主题对应的
时代、地域、社会环境和技术水平；除非剧本明确包含穿越或跨时代设定，否则禁止混入不属于该背景的元素。
角色视觉提示词还必须要求生成一张完整的角色设定板：规整多格排版，第一排为正面/严格侧面/背面
三视图，第二排为六个不同面部表情特写，第三排为四个不同的全身动作姿态；禁止左右二分构图，
禁止只生成头像加单张全身像，所有格子必须保持同一张脸、发型、服装、体型和配饰，不要文字、水印或多余人物。
""".strip()
