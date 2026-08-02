from src.llm_service.skills.base import BaseSkill


class ScriptDecomposerSkill(BaseSkill):
    name = "script_decomposer"
    description = "把短剧剧本拆成分集、分镜，以及可复用的角色、场景、道具提示词。"
    parameters = {
        "type": "object",
        "properties": {
            "script": {"type": "string", "description": "用户输入的短剧剧本"},
            "style": {"type": "string"},
            "theme": {"type": "string"},
            "ratio": {"type": "string"},
        },
        "required": ["script", "style", "theme", "ratio"],
        "additionalProperties": False,
    }
    instruction = """
输出严格 JSON：episodes、assets。每个 episode 包含 name 和 shots；每个 shot 包含
title、original_text、prompt。assets 中只能使用 character、scene、prop 三种 type，
每个素材必须有稳定的 name 和适合图像生成的 prompt。不要生成图片或视频 URL。
分镜要能直接用于后续的视频提示词生成，明确主体、动作、场景、镜头、光线和风格。
""".strip()
