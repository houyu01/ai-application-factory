//! One-time, operator-authorized generator for the bundled system voice sample assets.
//!
//! Run with the explicit path to a configured desktop SQLite database. It reads the saved audio
//! configuration without printing credentials, calls the configured Doubao TTS endpoint once per
//! system voice, and writes distributable MP3 files into `resources/voice_samples`.

use std::{env, fs, path::PathBuf};

use base64::Engine;
use reqwest::blocking::Client;
use rusqlite::Connection;
use serde_json::{json, Value};
use uuid::Uuid;

const ENDPOINT: &str = "https://openspeech.bytedance.com/api/v3/plan/tts/unidirectional";
const RESOURCE_ID: &str = "seed-tts-2.0";
const SAMPLE_TEXT: &str = "你好，很高兴在这个故事里与你相遇。";

struct Sample {
    id: &'static str,
    name: &'static str,
    gender: &'static str,
    prompt: &'static str,
    speaker: &'static str,
}

const SAMPLES: &[Sample] = &[
    Sample { id: "broken_whisper_resilient_female", name: "破碎感低语坚韧音（女）", gender: "女", prompt: "女声压低至耳语，气息微微发颤带着一触即碎的脆弱感，声线纤细单薄，看似摇摇欲坠，基底却绷着一股不肯妥协的韧劲，温柔易碎，又绝不示弱。", speaker: "zh_female_xiaohe_uranus_bigtts" },
    Sample { id: "cold_boss_male", name: "冷酷霸总音（男）", gender: "男", prompt: "成年男性低沉有磁性的声线，语速从容，语气冷静克制，字句带有不容置疑的掌控感。", speaker: "zh_male_m191_uranus_bigtts" },
    Sample { id: "cool_career_newcomer_male", name: "清冷职场新人音（男）", gender: "男", prompt: "年轻男性清透偏冷的声线，吐字清晰，语气礼貌而有距离感。", speaker: "zh_male_taocheng_uranus_bigtts" },
    Sample { id: "soft_puppy_boyfriend_male", name: "奶狗软萌男友音（男）", gender: "男", prompt: "年轻男性明亮柔软的声线，带有自然亲近感和轻微撒娇感，语气真诚直接。", speaker: "zh_male_taocheng_uranus_bigtts" },
    Sample { id: "sickly_gloomy_yandere_male", name: "病娇阴郁疯批音（男）", gender: "男", prompt: "男性偏低的阴郁声线，气息收紧，语调平静得近乎异常。", speaker: "zh_male_taocheng_uranus_bigtts" },
    Sample { id: "ruthless_old_fox_male", name: "狠戾流老狐狸音（男）", gender: "男", prompt: "成熟男性沙哑低沉的声线，语气老练圆滑，谈笑间带着试探和锋利感。", speaker: "zh_male_m191_uranus_bigtts" },
    Sample { id: "arrogant_genius_male", name: "傲慢天才狂气音（男）", gender: "男", prompt: "年轻男性清亮且张扬的声线，语速利落，语气自信。", speaker: "zh_male_taocheng_uranus_bigtts" },
    Sample { id: "cool_abstinent_detective_female", name: "清冷禁欲刑警音（女）", gender: "女", prompt: "成年女性清冷干净的声线，吐字利落，语气克制专业。", speaker: "zh_female_vv_uranus_bigtts" },
    Sample { id: "warm_older_brother_male", name: "温柔大哥哥音（男）", gender: "男", prompt: "成年男性温暖沉稳的声线，音色宽厚，语气耐心可靠。", speaker: "zh_male_m191_uranus_bigtts" },
    Sample { id: "sweet_cold_yandere_male", name: "甜冷病娇音（男）", gender: "男", prompt: "男性清甜柔和的声线中带着冷感，平静说话时显得亲昵温柔。", speaker: "zh_male_taocheng_uranus_bigtts" },
    Sample { id: "cold_royal_sister_female", name: "冷酷御姐音（女）", gender: "女", prompt: "成年女性低沉有力量的声线，语速干练，语气冷静果断。", speaker: "zh_female_vv_uranus_bigtts" },
    Sample { id: "strong_female_lead", name: "女强角色音（女）", gender: "女", prompt: "女性明亮坚定的声线，吐字清晰有力度，语气果断而有行动感。", speaker: "zh_female_xiaohe_uranus_bigtts" },
    Sample { id: "mature_warm_goddess_female", name: "成熟温柔女神音（女）", gender: "女", prompt: "成年女性柔和成熟的声线，音色细腻从容，语气温柔但不软弱。", speaker: "zh_female_xiaohe_uranus_bigtts" },
    Sample { id: "sweet_fox_tease_female", name: "绿茶甜心撒娇小狐狸音（女）", gender: "女", prompt: "年轻女性甜软灵动的声线，语气亲昵，尾音带一点撒娇和若有若无的试探。", speaker: "zh_female_xiaohe_uranus_bigtts" },
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database = env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("请传入已配置应用的 ai_application_factory.db 路径")?;
    let config = audio_config(&database)?;
    let api_key = config["api_key"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or("音频配置中没有 API Key")?;
    let endpoint = config["endpoint"]
        .as_str()
        .filter(|value| !value.is_empty())
        .unwrap_or(ENDPOINT);
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/voice_samples");
    fs::create_dir_all(&output)?;
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()?;
    for sample in SAMPLES {
        let body = client.post(endpoint).header("X-Api-Key", api_key).header("X-Api-Resource-Id", RESOURCE_ID).json(&json!({
            "user": { "uid": Uuid::new_v4().simple().to_string() },
            "req_params": {
                "text": SAMPLE_TEXT,
                "speaker": sample.speaker,
                "voice_instruction": format!("这是角色音色试听。音色标题：{}。角色性别：{}。声音设定：{}。请严格依据上述设定演绎，保持自然中文发音；不要朗读这些设定内容。", sample.name, sample.gender, sample.prompt),
                "audio_params": { "format": "mp3", "sample_rate": 24000 }
            }
        })).send()?.error_for_status()?.bytes()?;
        let audio = decode_audio(&body)?;
        fs::write(output.join(format!("{}.mp3", sample.id)), audio)?;
        println!("generated {}", sample.id);
    }
    Ok(())
}

fn audio_config(database: &PathBuf) -> Result<Value, Box<dyn std::error::Error>> {
    let connection = Connection::open(database)?;
    let raw = connection.query_row(
        "SELECT value_json FROM app_settings WHERE key='audio'",
        [],
        |row| row.get::<_, String>(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}

fn decode_audio(body: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut audio = Vec::new();
    for frame in serde_json::Deserializer::from_slice(body).into_iter::<Value>() {
        let frame = frame?;
        let code = frame["code"].as_i64().unwrap_or_default();
        if code != 0 && code != 20_000_000 {
            return Err(frame["message"]
                .as_str()
                .unwrap_or("豆包语音请求失败")
                .into());
        }
        if let Some(chunk) = frame["data"].as_str().filter(|value| !value.is_empty()) {
            audio.extend(base64::engine::general_purpose::STANDARD.decode(chunk)?);
        }
    }
    (!audio.is_empty())
        .then_some(audio)
        .ok_or_else(|| "豆包语音没有返回音频数据".into())
}
