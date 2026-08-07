"""Short-drama orchestration built on the discovered drama skills."""

from __future__ import annotations

import json
import math
import os
import re
from typing import Any

from .agents.drama_agent import DramaAgent
from .client.openai_chat_client import OpenAIChatClient
from .client.openai_client import OpenAICLient, OpenAIClientBaseOptions


def _script_planner():
    from .planner import ScriptPlanner
    return ScriptPlanner

class ScriptPlannerDecompositionMixin:
    """Behavior slice of _script_planner()."""

    @staticmethod
    def _decomposition_prompt(
        script: str,
        runtime: dict[str, Any],
        skill_context: dict[str, Any],
        asset_skill_context: dict[str, Any] | None = None,
    ) -> str:
        return (
            "你正在执行短剧初始化。请只返回合法 JSON，不要 Markdown。\n"
            "JSON 结构必须为：{\"episodes\":[{\"name\":\"第1集\",\"shots\":["
            "{\"title\":\"...\",\"original_text\":\"...\",\"prompt\":\"...\"}]}],"
            "\"assets\":[{\"type\":\"character|scene|prop\",\"name\":\"...\",\"prompt\":\"...\"}]}。\n"
            "请按剧情时间线均匀拆解：每集至少 2 个分镜，每个分镜只承载一个连续动作或一个明确的信息变化，"
            "建议每个分镜 20～80 个字、对应约 3～8 秒视频，并且能独立生成视频、又能和相邻镜头衔接。"
            "original_text 必须是当前分镜对应的短文本片段，不能复制完整剧本，不能让多个分镜重复同一段；"
            "需要把完整剧本中的事件按顺序分配到各个分镜，保证每个片段只出现一次。"
            "素材目录是独立的视觉设定，不是剧本原文摘抄。每种素材至少生成 5 个，"
            "角色、场景、道具总数不足时，要从剧情中的关系、地点、线索和行动合理扩展，"
            "但不能使用‘主要角色’、‘主要场景’、‘关键道具’等泛化名称。\n"
            "角色 name 必须直接使用简短、真实、方便观众记忆的人名，优先使用 2～4 个字的人名，"
            "例如‘林岩’、‘道玄’、‘苏晚’，不要把身份、职业、阵营或叙事功能写进 name，"
            "不要使用‘山村遗孤·少年剑修’、‘青云山师姐·引路人’这类代称或复合身份名；"
            "身份、职业、身世和角色功能必须写进 prompt。prompt 必须分成两段：第一段写身世、性格、年龄或角色功能；"
            "性格不能只写‘坚毅’‘温柔’等单个形容词，必须补充至少 3 个可观察的行为特征，例如待人方式、压力下的情绪反应、"
            "思考或决策习惯、对重要人物的态度、说话或行动倾向；第二段写年龄/性别、脸型、肤色、"
            "眉眼、发型、身型、衣料和服装等稳定视觉特征。\n"
            "场景 name 必须有真实地点含义，prompt 必须按‘场景名\n场景由来\n外形、色调颜色、"
            "场景中物品状态、整体氛围、人物与文字限制’组织，明确写出无人物、无背景文字等约束。\n"
            "道具 name 必须有真实叙事含义，prompt 必须写道具颜色、材质、细节、磨损、装饰和表面文字；"
            "不能把完整剧本或同一段故事复制到每个素材 prompt。\n"
            f"所有角色、场景、道具 prompt 第一行必须写‘叙述背景主题：{runtime.get('theme', '都市')}’，"
            "并严格约束服饰、建筑陈设、交通、照明、道具形制和制作工艺符合该背景的时代与技术水平；"
            "除非剧本明确包含穿越或跨时代设定，否则不得混入跨时代元素。\n"
            "素材只提取剧本真实出现且会复用的角色、场景、道具，并为后续图片生成补齐视觉细节。\n"
            f"配置：风格={runtime.get('style', '真人风格')}，题材={runtime.get('theme', '都市')}，"
            f"画幅={runtime.get('ratio', '9:16')}，分辨率={runtime.get('resolution', '720p')}，"
            f"分镜约束={json.dumps(runtime.get('shot_constraints') or {}, ensure_ascii=False)}。\n"
            f"分镜 Skill 执行结果：{json.dumps(skill_context, ensure_ascii=False)}\n"
            f"素材 Skill 执行结果：{json.dumps(asset_skill_context or {}, ensure_ascii=False)}\n"
            f"剧本：\n{script}"
        )

    def _fallback_shot_prompt(project: dict[str, Any], shot: dict[str, Any], assets: list[dict[str, Any]]) -> str:
        asset_lines = "；".join(f"{item.get('type')}：{item.get('name')}（{item.get('prompt')}）" for item in assets)
        return (
            f"场景：{asset_lines or '按照分镜原文建立环境'}\n"
            f"动作：{shot.get('original_text', '')}\n"
            f"风格：{project.get('style', '真人风格')}，画幅：{project.get('ratio', '9:16')}\n"
            "镜头：保持主体连续，平滑衔接前后镜头；光线和情绪服务于剧情。"
        )

    def _agent(self, runtime: dict[str, Any], context_value: str) -> DramaAgent | None:
        api_key = runtime.get("api_key") or os.getenv("OPENAI_API_KEY")
        if not api_key:
            return None
        options = OpenAIClientBaseOptions(
            api_key=api_key,
            base_url=runtime.get("endpoint") or runtime.get("base_url") or os.getenv("OPENAI_BASE_URL"),
            model=runtime.get("model") or os.getenv("OPENAI_MODEL", "gpt-4o-mini"),
        )
        provider = str(runtime.get("provider") or "ark").lower()
        client = OpenAIChatClient(options) if provider in {"dashscope", "tencent"} else OpenAICLient(options)
        return DramaAgent(llm_client=client, context={"drama": context_value})

    @staticmethod
    def _split_script_into_segments(script: str, count: int) -> list[str]:
        """Split a script into chronological, near-equal story beats.

        Sentence and clause punctuation are preferred as cut points. If the
        input contains one very long sentence, the final fallback cuts by
        character position so one shot still does not receive the whole plot.
        """

        text = _script_planner()._clean_script(script)
        if not text:
            return []
        count = max(1, min(int(count), len(text)))
        if count == 1:
            return [text]

        units = [
            match.group(0)
            for match in re.finditer(r".+?(?:[，,。！？!?；;]|$)", text)
            if match.group(0).strip()
        ]
        if len(units) >= count:
            cumulative: list[int] = []
            total = 0
            for unit in units:
                total += len(unit)
                cumulative.append(total)
            segments: list[str] = []
            start_unit = 0
            for index in range(count):
                remaining = count - index - 1
                min_end = start_unit + 1
                max_end = len(units) - remaining
                target = len(text) * (index + 1) / count
                end_unit = min(
                    range(min_end, max_end + 1),
                    key=lambda candidate: abs(cumulative[candidate - 1] - target),
                )
                segments.append("".join(units[start_unit:end_unit]).strip())
                start_unit = end_unit
            return [segment for segment in segments if segment]

        # There are fewer punctuation boundaries than requested shots. Keep
        # all source text while distributing the remaining characters evenly.
        segments = []
        start = 0
        for index in range(count):
            if index == count - 1:
                end = len(text)
            else:
                end = round(len(text) * (index + 1) / count)
            segment = text[start:end].strip()
            if segment:
                segments.append(segment)
            start = end
        return segments

    def _repair_shot_segments(
        episodes: list[dict[str, Any]], script: str, runtime: dict[str, Any]
    ) -> list[dict[str, Any]]:
        """Repair LLM output that repeats the complete script per shot.

        The model receives the full script as context, so a weak response can
        accidentally copy it into every ``original_text``. This post-process
        keeps the model's episode/shot structure, but redistributes the source
        story into non-overlapping chronological segments.
        """

        source = _script_planner()._clean_script(script)
        shot_refs = [
            (episode, shot)
            for episode in episodes
            for shot in episode.get("shots", [])
            if isinstance(shot, dict)
        ]
        if not shot_refs or not source:
            return episodes

        texts = [str(shot.get("original_text") or "").strip() for _, shot in shot_refs]
        full_script_count = sum(
            _script_planner()._is_full_script_like(text, source) for text in texts
        )
        compact_texts = {re.sub(r"\s+", "", text) for text in texts if text}
        duplicated = len(compact_texts) < max(1, int(len(texts) * 0.75))
        should_split = full_script_count >= max(1, int(len(texts) * 0.6)) or duplicated

        if len(shot_refs) == 1 and len(source) > 60:
            should_split = True
            desired_count = max(2, min(8, math.ceil(len(source) / 80)))
        else:
            desired_count = len(shot_refs)
        if not should_split:
            return episodes

        segments = _script_planner()._split_script_into_segments(source, desired_count)
        if not segments:
            return episodes

        if len(segments) > len(shot_refs):
            # A single model shot is not enough editorial granularity. Expand
            # it inside its existing episode instead of inventing new episodes.
            first_episode = episodes[0]
            template = shot_refs[0][1]
            expanded: list[dict[str, Any]] = []
            for index, segment in enumerate(segments, start=1):
                clone = dict(template)
                clone["id"] = f"{template.get('id') or 'shot'}_{index}"
                clone["title"] = f"分镜 {index}"
                clone["original_text"] = segment
                clone["prompt"] = ""
                expanded.append(clone)
            first_episode["shots"] = expanded
            shot_refs = [(first_episode, shot) for shot in expanded]

        titles = ["开场建立", "人物行动", "冲突推进", "信息揭示", "关系变化", "高潮转折", "结果显现", "收束结尾"]
        for index, ((_, shot), segment) in enumerate(zip(shot_refs, segments), start=1):
            previous_prompt = str(shot.get("prompt") or "").strip()
            shot["original_text"] = segment
            if not previous_prompt or _script_planner()._is_full_script_like(previous_prompt, source):
                title = titles[index - 1] if index <= len(titles) else f"分镜 {index}"
                shot["prompt"] = (
                    f"场景动作：{segment}\n"
                    f"镜头目标：{title}，保持动作连续并衔接前后分镜。\n"
                    f"风格：{runtime.get('style', '真人风格')}，画幅：{runtime.get('ratio', '9:16')}。"
                )
        return episodes

    @staticmethod
    def _fallback_plan(script: str, runtime: dict[str, Any]) -> dict[str, Any]:
        clean_script = _script_planner()._clean_script(script)
        segment_count = max(2, min(8, math.ceil(len(clean_script) / 80)))
        segments = _script_planner()._split_script_into_segments(clean_script, segment_count)
        titles = ["开场建立", "人物行动", "冲突发生", "信息揭示", "关系变化", "高潮推进", "结果显现", "收束结尾"]
        shots = []
        for index, segment in enumerate(segments, start=1):
            title = titles[index - 1] if index <= len(titles) else f"分镜 {index}"
            shots.append(
                {
                    "id": f"shot_{index:03d}",
                    "title": title,
                    "original_text": segment,
                    "prompt": (
                        f"{runtime.get('style', '真人风格')}，{title}，"
                        f"围绕这一段连续动作生成镜头：{segment}"
                    ),
                }
            )
        return {
            "episodes": [
                {
                    "name": "第1集",
                    "shots": shots,
                }
            ],
            "assets": _script_planner()._fallback_asset_catalog(clean_script, runtime),
        }

    @staticmethod
    def _normalize_character_name(name: str, index: int) -> str:
        """Keep character names human-readable and move identity into prompt."""

        candidate = re.sub(r"[（(].*?[）)]", "", str(name or "")).strip()
        candidate = re.split(r"[·•丨|｜/／—–-]", candidate, maxsplit=1)[0].strip()
        forbidden_markers = (
            "主要角色", "核心角色", "角色", "人物", "主角", "主人公", "男主", "女主",
            "少年", "少女", "遗孤", "剑修", "师姐", "师妹", "引路人", "同行者",
            "对手", "反派", "执棋者", "收养者", "行动者", "冲突制造者",
        )
        if (
            2 <= len(candidate) <= 8
            and re.fullmatch(r"[\u4e00-\u9fffA-Za-z][\u4e00-\u9fffA-Za-z]{1,7}", candidate)
            and not any(marker in candidate for marker in forbidden_markers)
        ):
            return candidate
        return ("林岩", "苏晚", "道玄", "沈砚", "楚宁", "顾宁", "叶青", "陆沉")[index % 8]

    @staticmethod
    def _fallback_asset_catalog(
        script: str, runtime: dict[str, Any]
    ) -> list[dict[str, Any]]:
        """Build a useful local asset catalog when the model is unavailable.

        This is intentionally a semantic fallback rather than a copy of the
        source script. It gives the UI multiple reusable visual entities and
        keeps each image prompt in the same structured format expected from
        the asset-prompt skill.
        """

        text = _script_planner()._clean_script(script)
        style = str(runtime.get("style") or "真人风格")
        theme = str(runtime.get("theme") or "都市")
        characters: list[tuple[str, str]] = []
        if _script_planner()._contains_any(text, ("男主", "主人公", "少年", "男孩")):
            characters.append(("林岩", "青年男性故事主人公，出身偏僻山村，背负故乡旧案并沿线索追查真相。"))
        if _script_planner()._contains_any(text, ("女主", "少女", "爱人", "女侠")):
            characters.append(("苏晚", "青年女性同行者，与主人公共同面对冲突并推动关系变化。"))
        if _script_planner()._contains_any(text, ("师姐", "师妹")):
            characters.append(("楚宁", "青年女性门内弟子，熟悉仙门规则并掌握关键线索，是主人公的引路者。"))
        if _script_planner()._contains_any(text, ("师父", "道长", "修仙者", "收养")):
            characters.append(("道玄", "中年男性修行者，曾收养并教导主人公，同时隐藏着上一代恩怨。"))
        if _script_planner()._contains_any(text, ("反派", "boss", "魁首", "魔道", "幕后")):
            characters.append(("沈砚", "成年男性关键对手，表面维护秩序，实际操纵冲突并隐藏幕后真相。"))
        if not characters:
            characters.append(("林岩", "青年男性故事主人公，主动寻找答案并推动剧情发展。"))
        if len(characters) < 2:
            characters.append(("沈砚", "成年男性关键对手，与主人公目标相反并持续制造阻力。"))
        characters = _script_planner()._unique_specs(characters)

        scenes: list[tuple[str, str, str, str, str]] = []
        scene_candidates = (
            (
                ("山村", "小村", "故乡"),
                "男主故乡山村·灭门旧址",
                "主人公幼年居住并遭遇家庭惨祸的偏僻山村。",
                "土坯民居、低矮篱笆和荒废田地围成狭窄村巷，残破门窗与焦黑梁木清晰可见。",
                "陈旧木桌、破陶罐、散落柴火和被踩坏的作物保持凌乱破损状态",
                "天色昏暗，冷灰与暗褐色调，空气中残留硝烟感，整体肃杀凄凉；无人物，无背景文字。",
            ),
            (
                ("车站", "月台", "列车"),
                "黄昏旧车站·失踪线索入口",
                "主人公发现第一条线索、准备追踪离去之人的旧车站。",
                "老式站房、斑驳站牌、铁轨和窄月台构成纵深，夕阳从站棚缝隙斜射进来。",
                "木质长椅、旧行李箱、褪色时刻牌和散落票据呈现被匆忙遗留的状态",
                "金橙夕照与深蓝阴影交错，氛围孤寂而紧迫；无人物，无背景文字。",
            ),
            (
                ("集市", "旧城", "城门"),
                "旧城集市·师姐重逢处",
                "主人公在旧城寻找线索并与故人重逢的公开场所。",
                "狭长石板街连接城门，摊棚、木牌和层叠屋檐形成拥挤但清晰的空间层次。",
                "布棚、竹筐、药草包和旧灯笼散布在摊位周围，物品有真实使用痕迹",
                "青灰建筑配少量暖色灯火，烟尘和人声留下喧闹余韵；无人物，无背景文字。",
            ),
            (
                ("密道", "地宫", "暗室"),
                "城门密道·真相藏匿处",
                "主人公凭借线索进入、最终接近真相的隐秘空间。",
                "潮湿石壁、狭窄台阶和拱形通道向黑暗深处延伸，局部有冷白光线从缝隙落下。",
                "锈蚀铁环、残旧火把、碎石和积水贴近墙边，长期无人使用但仍保留通行痕迹",
                "深灰、墨蓝和微弱冷光构成压迫氛围，安静且充满未知；无人物，无背景文字。",
            ),
        )
        for keywords, name, origin, appearance, objects, atmosphere in scene_candidates:
            if _script_planner()._contains_any(text, keywords):
                scenes.append((name, origin, appearance, objects, atmosphere))
        if len(scenes) < 2:
            scenes.extend(
                [
                    (
                        "故事开场地·旧居",
                        "主人公最初整理线索、建立行动目标的生活空间。",
                        "一间结构完整但略显陈旧的居所，木墙、窗格和窄桌形成明确室内纵深。",
                        "桌上放着旧书、油灯和收纳盒，物件按照长期生活习惯自然摆放",
                        "低饱和暖灰色调，安静克制并带有即将出发的悬念；无人物，无背景文字。",
                    ),
                    (
                        "冲突发生地·隐秘据点",
                        "对立双方交换信息、隐藏物证或发生正面冲突的关键空间。",
                        "封闭院落与半开木门形成前后景，墙面、台阶和遮挡物为人物行动提供层次。",
                        "破旧桌案、封存木箱、散落纸页和未熄灯盏呈现刚被翻动过的状态",
                        "冷暖光线冲突，氛围紧张压迫；无人物，无背景文字。",
                    ),
                ][: max(0, 2 - len(scenes))]
            )
        scenes = _script_planner()._unique_specs(scenes)

        props: list[tuple[str, str, str]] = []
        prop_candidates = (
            (("信", "书信"), "未署名泛黄信件·失踪线索载体", "纸张发黄卷边，墨迹褪色但关键字仍清晰，封口残留暗红蜡痕，无多余背景文字。"),
            (("钥匙", "锁"), "旧铜钥匙·密道开启凭证", "铜制钥匙表面有氧化斑和长期摩擦形成的亮痕，齿纹细密，钥匙柄刻有简化云纹，没有钥匙穗。"),
            (("剑", "刀"), "望云剑·师门信物", "剑身纯白泛冷光，剑柄为乌金材质，护手刻有浅色云纹，剑身靠近柄部留有‘望云’二字，剑穗缺失。"),
            (("车票",), "泛黄旧车票·父亲行踪凭证", "薄纸票面呈褐黄色，边缘起毛并有折痕，印刷字迹部分模糊，背面留有手写日期和短线索。"),
            (("玉佩", "令牌"), "青云山玉佩·身份凭证", "青白玉质，边缘有细小磕痕，正面浮雕云山纹样，背面刻着门派印记，系一根磨旧的素色绳。"),
        )
        for keywords, name, detail in prop_candidates:
            if _script_planner()._contains_any(text, keywords):
                props.append((name, detail, "剧情中的关键物证或行动凭证，负责连接人物关系与后续线索。"))
        if len(props) < 2:
            props.extend(
                [
                    (
                        "旧木盒·秘密保管匣",
                        "深棕木质，边角磨损发亮，铜扣有轻微锈迹，盒盖内侧留有浅色刻痕，内部适合存放纸张或小型信物。",
                        "用于保管关键线索和私人信物。",
                    ),
                    (
                        "冷青油灯·隐秘空间照明物",
                        "青铜灯座有烟熏痕迹，玻璃罩略有灰尘，灯芯燃烧稳定，底部带有简单回纹装饰，不出现品牌和背景文字。",
                        "用于强化隐秘场景的光线和行动氛围。",
                    ),
                ][: max(0, 2 - len(props))]
            )
        props = _script_planner()._unique_specs(props)

        assets: list[dict[str, Any]] = []
        for index, (name, identity) in enumerate(characters, start=1):
            assets.append(
                {
                    "id": f"char_{index:03d}",
                    "type": "character",
                    "name": name,
                    "prompt": _script_planner()._character_prompt(name, identity, style, index, theme),
                }
            )
        for index, (name, origin, appearance, objects, atmosphere) in enumerate(scenes, start=1):
            assets.append(
                {
                    "id": f"scene_{index:03d}",
                    "type": "scene",
                    "name": name,
                    "prompt": _script_planner()._scene_prompt(
                        name, origin, appearance, objects, atmosphere, theme
                    ),
                }
            )
        for index, (name, detail, purpose) in enumerate(props, start=1):
            assets.append(
                {
                    "id": f"prop_{index:03d}",
                    "type": "prop",
                    "name": name,
                    "prompt": (
                        f"{_script_planner()._asset_theme_constraint(theme, 'prop')}\n"
                        f"{name}\n颜色、材质与细节：{detail}\n故事作用：{purpose}"
                    ),
                }
            )
        return assets


    @staticmethod
    def _is_full_script_like(value: str, script: str) -> bool:
        candidate = re.sub(r"\s+", "", str(value or ""))
        source = re.sub(r"\s+", "", str(script or ""))
        if not candidate or not source:
            return not candidate
        return source in candidate or (
            candidate in source and len(candidate) >= max(24, int(len(source) * 0.72))
        )


    @staticmethod
    def _clean_script(script: str) -> str:
        return re.sub(r"\s+", " ", str(script or "")).strip()

    @staticmethod
    def _meaningful_asset_name(
        asset_type: str, name: str, script: str, index: int
    ) -> str:
        if asset_type == "character":
            return _script_planner()._normalize_character_name(name, index)
        if "·" in name or len(name) >= 7:
            return name
        if asset_type == "scene":
            suffixes = ("·剧情起点", "·线索入口", "·冲突空间")
        else:
            suffixes = ("·关键线索载体", "·行动凭证", "·身份信物")
        suffix = suffixes[index % len(suffixes)]
        return f"{name}{suffix}"
