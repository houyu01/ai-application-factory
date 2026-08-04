"""Built-in voice presets used by drama character assets."""

from __future__ import annotations


# The catalog is seeded into SQLite on first startup.  Keeping the seed data
# in the domain layer makes a fresh local database deterministic while still
# allowing the database rows to be queried by every application surface.
DEFAULT_VOICE_PRESETS: tuple[dict[str, str], ...] = (
    {
        "id": "none",
        "name": "不设置",
        "gender": "",
        "prompt": "",
    },
    {
        "id": "broken_whisper_resilient_female",
        "name": "破碎感低语坚韧音（女）",
        "gender": "女",
        "prompt": "女声压低至耳语，气息微微发颤带着一触即碎的脆弱感，声线纤细单薄，看似摇摇欲坠，基底却绷着一股不肯妥协的韧劲，温柔易碎，又绝不示弱。",
    },
    {
        "id": "cold_boss_male",
        "name": "冷酷霸总音（男）",
        "gender": "男",
        "prompt": "成年男性低沉有磁性的声线，语速从容，语气冷静克制，字句带有不容置疑的掌控感；情绪很少外露，但在亲近的人面前偶尔泄露压抑的温柔。",
    },
    {
        "id": "cool_career_newcomer_male",
        "name": "清冷职场新人音（男）",
        "gender": "男",
        "prompt": "年轻男性清透偏冷的声线，吐字清晰，语气礼貌而有距离感；初入职场时略显拘谨，遇到压力会短暂停顿思考，再用平稳的语气坚持自己的判断。",
    },
    {
        "id": "soft_puppy_boyfriend_male",
        "name": "奶狗软萌男友音（男）",
        "gender": "男",
        "prompt": "年轻男性明亮柔软的声线，带有自然亲近感和轻微撒娇感，语气真诚直接；面对喜欢的人会主动关心、容易害羞，遇到冲突时先安抚对方再表达自己的想法。",
    },
    {
        "id": "sickly_gloomy_yandere_male",
        "name": "病娇阴郁疯批音（男）",
        "gender": "男",
        "prompt": "男性偏低的阴郁声线，气息收紧，语调平静得近乎异常；表面温和有礼，字句里却藏着强烈的占有欲和不安全感，情绪失控前会刻意放慢语速。",
    },
    {
        "id": "ruthless_old_fox_male",
        "name": "狠戾流老狐狸音（男）",
        "gender": "男",
        "prompt": "成熟男性沙哑低沉的声线，语气老练圆滑，像总是留有余地；谈笑间带着试探和锋利感，真正做决定时果断狠戾，很少让对手听出真实意图。",
    },
    {
        "id": "arrogant_genius_male",
        "name": "傲慢天才狂气音（男）",
        "gender": "男",
        "prompt": "年轻男性清亮且张扬的声线，语速利落，语气自信甚至带有傲慢感；习惯直接指出问题、享受掌控局面，在真正认可的人面前会用行动代替夸奖。",
    },
    {
        "id": "cool_abstinent_detective_female",
        "name": "清冷禁欲刑警音（女）",
        "gender": "女",
        "prompt": "成年女性清冷干净的声线，吐字利落，语气克制专业，不带多余情绪；习惯先观察证据再下结论，面对危险保持冷静，对受害者和同伴流露出克制的关心。",
    },
    {
        "id": "warm_older_brother_male",
        "name": "温柔大哥哥音（男）",
        "gender": "男",
        "prompt": "成年男性温暖沉稳的声线，音色宽厚，语气耐心可靠；习惯先倾听再给建议，面对年轻人会自然照顾和安抚，真正需要承担责任时语气会变得坚定。",
    },
    {
        "id": "sweet_cold_yandere_male",
        "name": "甜冷病娇音（男）",
        "gender": "男",
        "prompt": "男性清甜柔和的声线中带着冷感，平静说话时显得亲昵温柔，情绪转暗时语调依旧轻缓却让人感到压迫；对在意的人格外执着，习惯把危险情绪藏在玩笑里。",
    },
    {
        "id": "cold_royal_sister_female",
        "name": "冷酷御姐音（女）",
        "gender": "女",
        "prompt": "成年女性低沉有力量的声线，语速干练，语气冷静果断，带有成熟的压迫感；习惯直接解决问题，不轻易求助，面对真正信任的人会用简短的话表达保护。",
    },
    {
        "id": "strong_female_lead",
        "name": "女强角色音（女）",
        "gender": "女",
        "prompt": "女性明亮坚定的声线，吐字清晰有力度，语气果断而有行动感；面对阻碍会迅速拆解问题、承担后果，不因他人的质疑退缩，情绪低落时也会维持表面的镇定。",
    },
    {
        "id": "mature_warm_goddess_female",
        "name": "成熟温柔女神音（女）",
        "gender": "女",
        "prompt": "成年女性柔和成熟的声线，音色细腻从容，语气温柔但不软弱；习惯耐心倾听和照顾他人的情绪，面对矛盾会先安抚再说出清晰坚定的立场。",
    },
    {
        "id": "sweet_fox_tease_female",
        "name": "绿茶甜心撒娇小狐狸音（女）",
        "gender": "女",
        "prompt": "年轻女性甜软灵动的声线，语气亲昵，尾音带一点撒娇和若有若无的试探；擅长用轻松玩笑掩饰真实目的，面对喜欢的人会主动示弱，也会敏锐观察对方反应。",
    },
)
