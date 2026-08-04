from src.llm_service.planner import ScriptPlanner


def _shots(plan: dict) -> list[dict]:
    return [shot for episode in plan["episodes"] for shot in episode["shots"]]


def test_fallback_plan_splits_long_script_into_distinct_story_beats():
    script = (
        "男主在山村小屋醒来，发现门外留下了一封没有署名的信。"
        "他沿着信中的线索赶往旧城，在集市上遇见多年未见的师姐。"
        "师姐告诉他，失踪的师父可能藏在城门内部，并交给他一枚旧钥匙。"
        "男主带着钥匙进入密道，终于看见了被隐藏多年的真相。"
    )

    plan = ScriptPlanner._fallback_plan(script, {"style": "真人风格", "ratio": "9:16"})
    shots = _shots(plan)

    assert len(shots) >= 2
    assert len({shot["original_text"] for shot in shots}) == len(shots)
    assert all(shot["original_text"] != script for shot in shots)
    assert "".join(shot["original_text"] for shot in shots) == script


def test_normalize_plan_repairs_repeated_full_script_per_shot():
    script = (
        "小林在黄昏车站捡到泛黄车票，随后发现车票背面写着失踪父亲的名字，"
        "他追着驶离的列车来到旧仓库，并在那里揭开了父亲留下的秘密。"
    )
    raw = {
        "episodes": [
            {
                "name": "第1集",
                "shots": [
                    {"title": "发现车票", "original_text": script, "prompt": script},
                    {"title": "追踪线索", "original_text": script, "prompt": script},
                    {"title": "揭开秘密", "original_text": script, "prompt": script},
                ],
            }
        ],
        "assets": [
            {"type": "character", "name": "小林", "prompt": "年轻男性"},
            {"type": "scene", "name": "车站", "prompt": "黄昏车站"},
        ],
    }

    plan = ScriptPlanner._normalize_plan(raw, script, {"style": "真人风格", "ratio": "9:16"})
    shots = _shots(plan)

    assert [shot["original_text"] for shot in shots] == ScriptPlanner._split_script_into_segments(script, 3)
    assert all(script not in shot["original_text"] for shot in shots)
    assert all("场景动作：" in shot["prompt"] for shot in shots)


def test_fallback_asset_catalog_has_multiple_semantic_assets_and_visual_prompts():
    script = (
        "男主从偏僻山村长大，灭门后被青云山道人收养，在仙门修炼多年。"
        "他在旧城遇见师姐，带着钥匙进入密道，最终揭露正道魁首的阴谋。"
    )

    plan = ScriptPlanner._fallback_plan(script, {"style": "真人风格", "ratio": "9:16"})
    assets = plan["assets"]

    assert all(sum(asset["type"] == asset_type for asset in assets) >= 2 for asset_type in ("character", "scene", "prop"))
    assert all(asset["name"] not in {"主要角色", "主要场景", "关键道具"} for asset in assets)
    assert all(script not in asset["prompt"] for asset in assets)
    character = next(asset for asset in assets if asset["type"] == "character")
    scene = next(asset for asset in assets if asset["type"] == "scene")
    prop = next(asset for asset in assets if asset["type"] == "prop")
    assert all(marker in character["prompt"] for marker in ("脸型", "肤色", "头发", "身型", "衣料"))
    assert "性格具体表现为" in character["prompt"]
    assert all(marker in character["prompt"] for marker in ("习惯", "遇到", "对信任的人"))
    character_names = [asset["name"] for asset in assets if asset["type"] == "character"]
    assert all("·" not in name for name in character_names)
    assert {"林岩", "道玄", "沈砚"}.issubset(set(character_names))
    assert all(marker in scene["prompt"] for marker in ("场景由来", "无人物", "无背景文字")) or "无人物" in scene["prompt"]
    assert all(marker in prop["prompt"] for marker in ("颜色", "材质", "纹理")) or "材质" in prop["prompt"]


def test_normalize_plan_converts_character_aliases_to_human_names():
    script = "男主从山村出发，师姐在旧城交给他一把钥匙。"
    raw = {
        "episodes": [{"name": "第1集", "shots": [{"title": "出发", "original_text": "男主从山村出发。"}]}],
        "assets": [
            {
                "type": "character",
                "name": "山村遗孤·少年剑修",
                "prompt": "出身山村的少年，性格坚毅，负责追查家族旧案。",
            },
            {
                "type": "character",
                "name": "青云山师姐·引路人",
                "prompt": "熟悉仙门规则的女性同行者，帮助主角寻找线索。",
            },
        ],
    }

    plan = ScriptPlanner._normalize_plan(raw, script, {"style": "真人风格", "ratio": "9:16"})
    characters = [asset for asset in plan["assets"] if asset["type"] == "character"]

    assert characters[0]["name"] == "林岩"
    assert characters[1]["name"] == "苏晚"
    assert all("·" not in asset["name"] for asset in characters)
    assert all(marker in characters[0]["prompt"] for marker in ("身世、身份与性格", "脸型", "衣料"))
    assert "性格具体表现为" in characters[0]["prompt"]
    assert "反复沉思" in characters[0]["prompt"]


def test_normalize_plan_repairs_generic_single_asset_output():
    script = "男主在山村发现一封信，随后带着钥匙进入密道，最终揭露正道魁首的秘密。"
    raw = {
        "episodes": [{"name": "第1集", "shots": [{"title": "开场", "original_text": script}]}],
        "assets": [
            {"type": "character", "name": "主要角色", "prompt": script},
            {"type": "scene", "name": "主要场景", "prompt": script},
            {"type": "prop", "name": "关键道具", "prompt": script},
        ],
    }

    plan = ScriptPlanner._normalize_plan(raw, script, {"style": "真人风格", "ratio": "9:16"})

    assert all(sum(asset["type"] == asset_type for asset in plan["assets"]) >= 2 for asset_type in ("character", "scene", "prop"))
    assert all(script not in asset["prompt"] for asset in plan["assets"])
    assert all("主要角色" != asset["name"] for asset in plan["assets"])
    assert all("主要场景" != asset["name"] for asset in plan["assets"])
    assert all("关键道具" != asset["name"] for asset in plan["assets"])


def test_fallback_shot_prompt_omits_subtitle_content_when_disabled():
    project = {
        "style": "真人风格",
        "shot_constraints": {"subtitles": False, "background_music": False},
    }
    shot = {"title": "旧城追查", "original_text": "林岩带着旧钥匙走进密道。"}
    assets = [
        {"id": "scene-1", "type": "scene", "name": "旧城密道"},
        {"id": "character-1", "type": "character", "name": "林岩"},
        {"id": "prop-1", "type": "prop", "name": "旧钥匙"},
    ]

    nodes = ScriptPlanner._fallback_shot_prompt_rich(project, shot, assets)
    prompt = ScriptPlanner.rich_prompt_to_text(nodes)

    assert "字幕" not in prompt
    assert "配音" in prompt
    assert "背景音乐" not in prompt


def test_generated_prompt_cleanup_removes_subtitle_blocks_but_keeps_voice():
    nodes = [
        {"type": "text", "text": "场景：旧城密道\n字幕：林岩走进密道\n"},
        {"type": "text", "text": "【配音：旁白｜VoiceID：沉稳男声｜台词：继续前进】\n"},
        {"type": "text", "text": "【字幕说明：画面底部显示台词】\n"},
        {"type": "text", "text": "约束：不要字幕；不要背景音乐。"},
    ]

    cleaned = ScriptPlanner._remove_disallowed_subtitle_sections(nodes, False)
    prompt = ScriptPlanner.rich_prompt_to_text(cleaned)

    assert "字幕" not in prompt
    assert "【配音：旁白" in prompt
    assert "不要背景音乐" in prompt


def test_generated_prompt_cleanup_removes_music_blocks_but_keeps_audio():
    nodes = [
        {"type": "text", "text": "场景：旧城密道\n背景音乐：低沉弦乐\n"},
        {"type": "text", "text": "【配音：旁白｜VoiceID：沉稳男声｜台词：继续前进】\n"},
        {"type": "text", "text": "【BGM：悬疑鼓点】\n音效：钥匙碰撞声\n环境音：滴水声\n"},
        {"type": "text", "text": "约束：不要背景音乐；需要字幕。"},
    ]

    cleaned = ScriptPlanner._remove_disallowed_music_sections(nodes, False)
    prompt = ScriptPlanner.rich_prompt_to_text(cleaned)

    assert "背景音乐" not in prompt
    assert "BGM" not in prompt
    assert "【配音：旁白" in prompt
    assert "音效：钥匙碰撞声" in prompt
    assert "环境音：滴水声" in prompt
    assert "需要字幕" in prompt
