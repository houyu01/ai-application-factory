from typing import Any

from src.llm_service.skills.base import BaseSkill, SkillContext


class ShotPromptGeneratorSkill(BaseSkill):
    """Skill that formats a shot into rich text and reference-image nodes."""
    name = "shot_prompt_generator"
    description = "将分镜文本和已保存的角色、场景、道具素材融合成带图片引用节点的视频生成富文本 Prompt。"
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
            "resolution": {"type": "string"},
            "shot_constraints": {
                "type": "object",
                "properties": {
                    "subtitles": {"type": "boolean"},
                    "background_music": {"type": "boolean"},
                },
                "additionalProperties": False,
            },
            "prompt_template_version": {"type": "string"},
        },
        "required": [
            "project_name",
            "shot_title",
            "storyboard_text",
            "assets",
            "style",
            "ratio",
            "duration",
            "resolution",
            "shot_constraints",
            "prompt_template_version",
        ],
        "additionalProperties": False,
    }
    instruction = """
    输出富文本 Prompt 的 nodes JSON，不要输出 Markdown 或解释。nodes 只能由 text 和 reference
    两种节点组成。text 节点保存可编辑文字；reference 节点必须包含已保存素材 catalog 中的
    asset_id、asset_type 和 label，用于在编辑器中渲染成带缩略图的 @图N（素材名）引用胶囊。
    严格按“场景、角色、风格、光线、位置、镜头、配音”组织文字。场景、角色、道具和占位图段落先列出
    实际使用的 reference 节点；位置和每个镜头的动作中再次引用发生交互的素材。
    每个分镜 Prompt 生成 2～3 个连续镜头，镜头必须使用“【镜头N | 时长Xs | 时间：日 外】”开头，
    并紧跟一段“【配音：旁白｜VoiceID：...｜状态：...｜情绪：...｜语气特点：...｜台词：...】”。
    需要使用图片的地方必须输出 reference 节点，不要生硬拼接字段。
    必须优先使用已保存素材的描述，不要臆造不存在的人物、地点、道具或占位图；要交代镜头起始状态
    和结束状态，保证相邻分镜可衔接。图片引用必须使用 reference 节点，不能把 image_url 写进 text。
    如果角色 catalog 提供了 voice 和 voice_prompt，必须在配音段落中写出该角色的 VoiceID 音色名称，
    并补充状态、情绪、语气特点和台词；不要把音色描述误当成图片 reference。同步遵守给定的分辨率、字幕和背景音乐约束。
    当 subtitles 为 false 时，绝不输出字幕段落、字幕说明、字幕标记或“不要字幕”文字；配音段落仍需保留。
    当 background_music 为 false 时，绝不输出背景音乐、配乐、BGM 段落或“不要背景音乐”文字；配音、音效和环境音仍需保留。
""".strip()

    def execute(self, arguments: dict[str, Any], context: SkillContext) -> dict[str, Any]:
        """Adapt the planning instruction to the selected persisted template."""

        result = super().execute(arguments, context)
        if arguments.get("prompt_template_version") == "v2":
            result["instruction"] = self.instruction.replace(
                "每个分镜 Prompt 生成 2～3 个连续镜头",
                "每个分镜 Prompt 只生成 1 个完整的连续长镜头，不要拆分镜头",
            )
        return result
