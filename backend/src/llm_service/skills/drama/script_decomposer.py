from typing import Any

from src.llm_service.skills.base import BaseSkill, SkillContext


class ScriptDecomposerSkill(BaseSkill):
    """Skill that divides a script into episode-aware shot skeletons."""
    name = "script_decomposer"
    description = "把短剧剧本拆成分集、分镜，以及可复用的角色、场景、道具提示词。"
    parameters = {
        "type": "object",
        "properties": {
            "script": {"type": "string", "description": "用户输入的短剧剧本"},
            "style": {"type": "string"},
            "theme": {"type": "string"},
            "ratio": {"type": "string"},
            "resolution": {"type": "string", "description": "视频分辨率，例如 720p 或 480p"},
            "shot_script_max_chars": {"type": "integer", "description": "每个分镜剧本文字上限"},
            "shot_constraints": {
                "type": "object",
                "properties": {
                    "subtitles": {"type": "boolean"},
                    "background_music": {"type": "boolean"},
                },
                "additionalProperties": False,
            },
        },
        "required": [
            "script",
            "style",
            "theme",
            "ratio",
            "resolution",
            "shot_script_max_chars",
            "shot_constraints",
        ],
        "additionalProperties": False,
    }
    instruction = """
输出严格 JSON：episodes、assets。每个 episode 包含 name 和 shots；每个 shot 包含
title、original_text、prompt。assets 中只能使用 character、scene、prop 三种 type，
每种素材至少生成 2 个，每个素材必须有稳定的、带真实含义的 name 和适合图像生成的独立 prompt。
角色 name 必须是简短真实的人名，例如“林岩”“道玄”“苏晚”；身份、职业、阵营和叙事功能只能放在角色 prompt 中；
角色性格不能只写一个形容词，必须补充至少 3 个可观察的行为特征，覆盖待人方式、压力下的情绪反应、思考或决策习惯、
对重要人物的态度、说话或行动倾向中的至少三项，
禁止使用“山村遗孤·少年剑修”这类代称。
不要生成“主要角色”“主要场景”“关键道具”等泛化素材，也不要生成图片或视频 URL。素材 prompt
必须遵循 asset_prompt_generator 的格式，不能把完整剧本复制到每个角色、场景或道具 prompt。
每个角色、场景、道具 prompt 第一行必须写“叙述背景主题：<theme 参数原文>”，并保证角色服饰、
场景建筑与陈设、道具形制与工艺符合该背景的时代、地域和技术水平；剧本未明确穿越时禁止混入跨时代元素。
请按照剧情时间线均匀拆解剧本：每个分镜只表达一个连续动作或一个明确的信息变化，
每个分镜剧本文字不得超过{shot_script_max_chars}字、约 3～8 秒视频。original_text 只能填写当前分镜的短文本片段，
禁止把完整剧本复制到每个分镜，禁止多个分镜重复同一段；完整剧本中的事件要按顺序分配到各个分镜。
分镜要能直接用于后续的视频提示词生成，明确主体、动作、场景、镜头、光线、风格、分辨率和字幕/背景音乐约束。
遇到“【第001集：集名】”格式的长剧正文时，必须按既有集号拆成独立 episode，不能重新合并；
每集生成2至4条按时间顺序衔接的分镜。每条 prompt 均使用富文本分段结构：
“场景：\n@图1（场景）\n\n角色：\n@图2（角色）\n\n道具：\n@图3（道具）\n\n风格：…\n光线：…\n位置：…\n\n【镜头1 | …】…”。
引用可先保留为待生成素材，禁止把整集、整部剧或原始用户剧本直接填入 prompt。
""".strip()

    def execute(self, arguments: dict[str, Any], context: SkillContext) -> dict[str, Any]:
        """Apply the saved storyboard-script ceiling to the decomposition instruction."""

        result = super().execute(arguments, context)
        result["instruction"] = self.instruction.format(
            shot_script_max_chars=int(arguments["shot_script_max_chars"])
        )
        return result
