//! Volcengine Speech V3 request defaults, legacy migration, and HTTP stream decoding.

use base64::Engine;
use serde_json::{json, Map, Value};

use crate::error::{AppError, AppResult};

/// The Agent Plan HTTP endpoint for Doubao Seed-TTS 2.0 non-streaming input.
pub(crate) const HTTP_ENDPOINT: &str =
    "https://openspeech.bytedance.com/api/v3/plan/tts/unidirectional";
/// Resource identifier for the Doubao Seed-TTS 2.0 character-billed model.
pub(crate) const RESOURCE_ID: &str = "seed-tts-2.0";
/// A generally available Seed-TTS 2.0 female speaker used when no gender is known.
pub(crate) const DEFAULT_FEMALE_SPEAKER: &str = "zh_female_vv_uranus_bigtts";
/// A generally available Seed-TTS 2.0 male speaker used for male catalog voices.
pub(crate) const DEFAULT_MALE_SPEAKER: &str = "zh_male_m191_uranus_bigtts";

/// Normalize V3 TTS settings while preserving the creator-selected model resource identifier.
pub(crate) fn apply_seed_tts_two_defaults(profile: &mut Map<String, Value>) {
    let has_endpoint = profile
        .get("endpoint")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty());
    if !has_endpoint {
        profile.insert("endpoint".to_owned(), json!(HTTP_ENDPOINT));
    }
    let model = profile
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(RESOURCE_ID)
        .to_owned();
    let mut models = profile
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .fold(Vec::new(), |mut values, value| {
            if !values.iter().any(|saved| saved == value) {
                values.push(value.to_owned());
            }
            values
        });
    if !models.contains(&model) {
        models.insert(0, model.clone());
    }
    profile.insert("model".to_owned(), json!(model));
    profile.insert("models".to_owned(), json!(models));
    profile.remove("app_id");
    profile.remove("resource_id");
    profile.remove("voice");
    profile.remove("create_url");
    profile.remove("query_url");
}

/// Convert the retired async-TTS profile while retaining its usable key and queue setting.
pub(crate) fn migrate_legacy_async_profile(profile: &mut Map<String, Value>) -> bool {
    let legacy_resource = profile
        .get("resource_id")
        .and_then(Value::as_str)
        .is_some_and(|value| value == "volc.tts_async.default");
    let legacy_model = profile
        .get("model")
        .and_then(Value::as_str)
        .is_some_and(|value| value == "volc.tts_async.default");
    let legacy_endpoint = profile
        .get("create_url")
        .and_then(Value::as_str)
        .is_some_and(|value| value.contains("/api/v1/tts_async/"));
    if !(legacy_resource || legacy_model || legacy_endpoint) {
        return false;
    }
    profile.remove("model");
    profile.remove("models");
    apply_seed_tts_two_defaults(profile);
    true
}

/// Choose a compatible built-in speaker without exposing provider ids in Settings.
pub(crate) fn speaker_for_voice(voice_id: Option<&str>, gender: &str) -> &'static str {
    match voice_id {
        Some("broken_whisper_resilient_female" | "mature_warm_goddess_female") => {
            "zh_female_xiaohe_uranus_bigtts"
        }
        Some("cool_abstinent_detective_female" | "cold_royal_sister_female") => {
            "zh_female_vv_uranus_bigtts"
        }
        Some("strong_female_lead" | "sweet_fox_tease_female") => "zh_female_xiaohe_uranus_bigtts",
        Some("cold_boss_male" | "ruthless_old_fox_male" | "warm_older_brother_male") => {
            DEFAULT_MALE_SPEAKER
        }
        Some(
            "cool_career_newcomer_male"
            | "soft_puppy_boyfriend_male"
            | "sickly_gloomy_yandere_male"
            | "arrogant_genius_male"
            | "sweet_cold_yandere_male",
        ) => "zh_male_taocheng_uranus_bigtts",
        _ if gender.trim() == "男" => DEFAULT_MALE_SPEAKER,
        _ => DEFAULT_FEMALE_SPEAKER,
    }
}

/// Build the complete natural-language style directive from catalog metadata.
///
/// The title, gender, and description deliberately remain out of spoken text: they steer Seed-TTS 2.0
/// through `voice_instruction`, while every catalog item speaks the same neutral preview sentence.
pub(crate) fn voice_style_instruction(name: &str, gender: &str, prompt: &str) -> String {
    let title = name.trim();
    let gender = gender.trim();
    let description = prompt.trim();
    format!(
        "这是角色音色试听。音色标题：{title}。角色性别：{}。声音设定：{}。请严格依据上述设定演绎，保持自然中文发音；不要朗读这些设定内容。",
        if gender.is_empty() { "未标注" } else { gender },
        if description.is_empty() { "自然、清晰、适合剧情台词" } else { description },
    )
}

/// Build the V3 HTTP request body expected by the single-input, chunked-output endpoint.
pub(crate) fn unidirectional_payload(
    user_id: &str,
    text: &str,
    speaker: &str,
    voice_instruction: &str,
) -> Value {
    let mut payload = json!({
        "user": { "uid": user_id },
        "req_params": {
            "text": text,
            "speaker": speaker,
            "audio_params": { "format": "mp3", "sample_rate": 24000 }
        }
    });
    if !voice_instruction.is_empty() {
        payload["req_params"]["voice_instruction"] = json!(voice_instruction);
    }
    payload
}

/// Decode concatenated JSON chunks from V3 HTTP into one playable MP3 byte stream.
pub(crate) fn decode_http_audio_chunks(body: &[u8]) -> AppResult<Vec<u8>> {
    let mut audio = Vec::new();
    let mut received_frame = false;
    for frame in serde_json::Deserializer::from_slice(body).into_iter::<Value>() {
        let frame =
            frame.map_err(|error| AppError::External(format!("火山引擎音频响应无效：{error}")))?;
        received_frame = true;
        let code = frame["code"].as_i64().unwrap_or_default();
        if code != 0 && code != 20_000_000 {
            return Err(AppError::External(format!(
                "火山引擎音频请求失败：{}",
                frame["message"].as_str().unwrap_or("未知错误")
            )));
        }
        if let Some(chunk) = frame["data"].as_str().filter(|value| !value.is_empty()) {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(chunk)
                .map_err(|_| {
                    AppError::External("火山引擎音频响应包含无效 Base64 数据".to_owned())
                })?;
            audio.extend(bytes);
        }
    }
    if !received_frame || audio.is_empty() {
        return Err(AppError::External(
            "火山引擎音频模型没有返回音频数据".to_owned(),
        ));
    }
    Ok(audio)
}

#[cfg(test)]
mod tests {
    use super::{
        apply_seed_tts_two_defaults, decode_http_audio_chunks, migrate_legacy_async_profile,
        speaker_for_voice, unidirectional_payload, voice_style_instruction, DEFAULT_FEMALE_SPEAKER,
        DEFAULT_MALE_SPEAKER, HTTP_ENDPOINT, RESOURCE_ID,
    };
    use serde_json::{json, Map};

    #[test]
    fn legacy_async_profile_keeps_key_and_switches_to_seed_tts_two() {
        let mut profile = Map::from_iter([
            ("app_id".to_owned(), json!("123")),
            ("api_key".to_owned(), json!("token")),
            ("resource_id".to_owned(), json!("volc.tts_async.default")),
            ("voice".to_owned(), json!("BV001_streaming")),
        ]);

        assert!(migrate_legacy_async_profile(&mut profile));
        assert_eq!(profile["endpoint"], HTTP_ENDPOINT);
        assert_eq!(profile["api_key"], "token");
        assert!(profile.get("app_id").is_none());
        assert!(profile.get("resource_id").is_none());
        assert!(profile.get("voice").is_none());
    }

    #[test]
    fn seed_tts_configuration_defaults_model_and_discards_unneeded_vendor_ids() {
        let mut profile = Map::from_iter([
            ("app_id".to_owned(), json!("123")),
            ("resource_id".to_owned(), json!(RESOURCE_ID)),
            ("voice".to_owned(), json!(DEFAULT_FEMALE_SPEAKER)),
            ("endpoint".to_owned(), json!("")),
        ]);

        apply_seed_tts_two_defaults(&mut profile);

        assert_eq!(profile["endpoint"], HTTP_ENDPOINT);
        assert_eq!(profile["model"], RESOURCE_ID);
        assert_eq!(profile["models"], json!([RESOURCE_ID]));
        assert!(profile.get("app_id").is_none());
        assert!(profile.get("resource_id").is_none());
        assert!(profile.get("voice").is_none());
    }

    #[test]
    fn seed_tts_configuration_preserves_a_creator_selected_model() {
        let mut profile = Map::from_iter([
            ("model".to_owned(), json!("seed-tts-2.0-custom")),
            (
                "models".to_owned(),
                json!(["seed-tts-2.0", "seed-tts-2.0-custom"]),
            ),
        ]);

        apply_seed_tts_two_defaults(&mut profile);

        assert_eq!(profile["model"], "seed-tts-2.0-custom");
        assert_eq!(
            profile["models"],
            json!(["seed-tts-2.0", "seed-tts-2.0-custom"])
        );
    }

    #[test]
    fn v3_http_chunks_are_joined_into_one_audio_stream() {
        let audio = decode_http_audio_chunks(
            br#"{"code":0,"data":"YQ=="}{"code":0,"data":"Yg=="}{"code":20000000,"message":"OK","data":null}"#,
        )
        .expect("decode chunks");

        assert_eq!(audio, b"ab");
    }

    #[test]
    fn voice_style_uses_all_catalog_fields_and_gender_appropriate_speaker() {
        let instruction = voice_style_instruction(
            "冷酷霸总音（男）",
            "男",
            "成年男性低沉有磁性的声线，语气冷静克制。",
        );
        let payload = unidirectional_payload(
            "test",
            "你好，很高兴与你相遇。",
            speaker_for_voice(Some("cold_boss_male"), "男"),
            &instruction,
        );

        assert!(instruction.contains("冷酷霸总音（男）"));
        assert!(instruction.contains("角色性别：男"));
        assert!(instruction.contains("低沉有磁性"));
        assert_eq!(payload["req_params"]["speaker"], DEFAULT_MALE_SPEAKER);
        assert_eq!(payload["req_params"]["voice_instruction"], instruction);
        assert_ne!(speaker_for_voice(None, "男"), DEFAULT_FEMALE_SPEAKER);
    }

    #[test]
    fn seed_tts_two_catalog_never_uses_the_mismatched_jupiter_speakers() {
        for voice_id in [
            "broken_whisper_resilient_female",
            "cold_boss_male",
            "cool_career_newcomer_male",
            "soft_puppy_boyfriend_male",
            "sickly_gloomy_yandere_male",
            "ruthless_old_fox_male",
            "arrogant_genius_male",
            "cool_abstinent_detective_female",
            "warm_older_brother_male",
            "sweet_cold_yandere_male",
            "cold_royal_sister_female",
            "strong_female_lead",
            "mature_warm_goddess_female",
            "sweet_fox_tease_female",
        ] {
            assert!(speaker_for_voice(Some(voice_id), "").ends_with("_uranus_bigtts"));
        }
    }
}
