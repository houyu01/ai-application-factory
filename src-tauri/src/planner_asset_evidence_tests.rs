use super::asset_evidence::AssetEvidence;

#[test]
fn screenshot_screenplay_has_only_grounded_names() {
    let script = "场景：村口石碑 正午 晴\n（竖屏中景）蝉鸣裹着热风刮过连片瓜田，红漆\"西瓜村\"石碑旁，林小满蹲在田埂啃西瓜。\n【人物首次出现：林小满｜人物描述：回村考公的林家孙女】\n山坳口走来拎牛皮纸袋的沈清和。\n【人物首次出现：沈清和｜人物描述：城里来的寻根青年】\n\"站住！\"陈大强举着沾保红袖。";
    let evidence = AssetEvidence::from_script(script);

    assert_eq!(evidence.names("character"), ["林小满", "沈清和", "陈大强"]);
    assert_eq!(evidence.names("scene"), ["村口石碑", "田埂"]);
    assert_eq!(evidence.names("prop"), ["牛皮纸袋", "西瓜"]);
    assert_eq!(
        evidence.canonical_name("character", "林小满（女主）", "林小满蹲在田埂啃西瓜"),
        Some("林小满".to_owned())
    );
    assert_eq!(
        evidence.canonical_name("scene", "外来灾星", "\"站住！\"陈大强举着"),
        None
    );
}

#[test]
fn dialogue_and_action_context_do_not_turn_prose_into_assets() {
    let script = "场景：市局审讯室·夜\n苏晚：把录音机放到桌上。\n顾北推门进来，握着证物袋。\n旁白：我知道清楚地记得那一夜。";
    let evidence = AssetEvidence::from_script(script);

    assert_eq!(evidence.names("character"), ["苏晚", "顾北"]);
    assert_eq!(evidence.names("scene"), ["市局审讯室"]);
    assert_eq!(evidence.names("prop"), ["录音机"]);
    assert!(!evidence
        .names("character")
        .iter()
        .any(|name| name == "我知" || name == "清地"));
}

#[test]
fn action_words_do_not_promote_ordinary_phrases_to_character_names() {
    let script = "场景：旧花园 夜\n苏醒后冲向门外，白天的花园里没有人说话。";
    let evidence = AssetEvidence::from_script(script);

    assert!(evidence.names("character").is_empty());
    assert_eq!(evidence.names("scene"), ["旧花园"]);
}

#[test]
fn supports_fantasy_and_modern_script_labels() {
    let script = "【场景：青云山演武场·日】\n林砚握紧长剑。\n大师兄走到廊下，递给林砚一枚令牌。\n地点：城南旧站房 夜\n陈警官拿起照片。";
    let evidence = AssetEvidence::from_script(script);

    assert!(evidence.names("character").contains(&"林砚".to_owned()));
    assert!(evidence.names("character").contains(&"大师兄".to_owned()));
    assert!(evidence.names("character").contains(&"陈警官".to_owned()));
    assert_eq!(evidence.names("scene"), ["青云山演武场", "城南旧站房"]);
    assert!(evidence.names("prop").contains(&"长剑".to_owned()));
    assert!(evidence.names("prop").contains(&"令牌".to_owned()));
    assert!(evidence.names("prop").contains(&"照片".to_owned()));
}

#[test]
fn explicit_prop_lists_keep_only_concrete_nouns() {
    let script = "道具：你、爷爷、张磊、围观村民、铜烟袋、竹筐、蜜瓜、你背包里的农科站检测报告、石墩\n物品：你、张磊、爷爷、村支书李叔、蜜瓜样品、名片、茶缸、西瓜种植手册、吊扇\n爷爷拿起铜烟袋。";
    let evidence = AssetEvidence::from_script(script);

    assert_eq!(
        evidence.names("prop"),
        [
            "铜烟袋",
            "竹筐",
            "蜜瓜",
            "农科站检测报告",
            "石墩",
            "蜜瓜样品",
            "名片",
            "茶缸",
            "西瓜种植手册",
            "吊扇",
        ]
    );
    assert_eq!(
        evidence.canonical_name(
            "prop",
            "你、爷爷、铜烟袋",
            "道具：你、爷爷、张磊、围观村民、铜烟袋、竹筐、蜜瓜、你背包里的农科站检测报告、石墩"
        ),
        None
    );
    assert_eq!(
        evidence.canonical_name("prop", "爷爷的铜烟袋", "爷爷拿起铜烟袋。"),
        Some("铜烟袋".to_owned())
    );
}
