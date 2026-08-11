//! Configured TTS provider adapters used only by durable voice-preview tasks.

use base64::Engine;
use reqwest::header::AUTHORIZATION;
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    volcengine_tts::{
        decode_http_audio_chunks, speaker_for_voice, unidirectional_payload,
        voice_style_instruction, HTTP_ENDPOINT, RESOURCE_ID,
    },
};

use super::{find_base64, find_url, model_for, ProviderClient};

impl ProviderClient {
    /// Generate and locally persist one catalog sample with the audio model selected in Settings.
    ///
    /// Voice-preview workers call this after storing their task, so a restart can safely replay any failed request.
    pub(crate) fn synthesize_voice_sample(
        &self,
        text: &str,
        voice_id: Option<&str>,
        name: &str,
        gender: &str,
        prompt: &str,
    ) -> AppResult<String> {
        let config = self.config("audio")?;
        let model = model_for(&config, None);
        if model.is_empty() {
            return Err(AppError::BadRequest("音频模型尚未配置模型名称".to_owned()));
        }
        let voice_instruction = voice_style_instruction(name, gender, prompt);
        if config["provider"].as_str().unwrap_or("ark") == "ark" {
            return self.synthesize_ark_audio(
                &config,
                text,
                speaker_for_voice(voice_id, gender),
                &voice_instruction,
            );
        }
        let response = match config["provider"].as_str().unwrap_or("ark") {
            "dashscope" => {
                self.synthesize_dashscope_audio(&config, &model, text, &voice_instruction)?
            }
            "tencent" => self.synthesize_tencent_audio(&config, text)?,
            _ => unreachable!("Ark audio was returned above"),
        };
        if let Some(url) = audio_url(&response) {
            return self.media.save_url(url, ".mp3");
        }
        if let Some(data) = audio_base64(&response) {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|_| AppError::External("音频模型返回了无效 base64".to_owned()))?;
            return self.media.save(&bytes, ".mp3", "audio/mpeg");
        }
        Err(AppError::External(
            "音频模型没有返回音频 URL 或 Base64".to_owned(),
        ))
    }

    fn synthesize_ark_audio(
        &self,
        config: &Map<String, Value>,
        text: &str,
        speaker: &str,
        voice_instruction: &str,
    ) -> AppResult<String> {
        let bytes = self.synthesize_ark_audio_bytes(config, text, speaker, voice_instruction)?;
        self.media.save(&bytes, ".mp3", "audio/mpeg")
    }

    /// Call the Doubao Seed-TTS 2.0 V3 HTTP endpoint and combine its JSON audio frames.
    pub(crate) fn synthesize_ark_audio_bytes(
        &self,
        config: &Map<String, Value>,
        text: &str,
        speaker: &str,
        voice_instruction: &str,
    ) -> AppResult<Vec<u8>> {
        let key = required(config, "api_key", "豆包语音模型需要配置 API Key")?;
        let endpoint = config["endpoint"]
            .as_str()
            .filter(|value| !value.is_empty())
            .unwrap_or(HTTP_ENDPOINT);
        let response = self
            .client
            .post(endpoint)
            .header("X-Api-Key", key)
            .header("X-Api-Resource-Id", RESOURCE_ID)
            .json(&unidirectional_payload(
                &Uuid::new_v4().simple().to_string(),
                text,
                speaker,
                voice_instruction,
            ))
            .send()
            .map_err(|error| AppError::External(format!("火山引擎音频请求失败：{error}")))?
            .error_for_status()
            .map_err(|error| AppError::External(format!("火山引擎音频请求失败：{error}")))?;
        let body = response
            .bytes()
            .map_err(|error| AppError::External(format!("火山引擎音频响应无效：{error}")))?;
        decode_http_audio_chunks(&body)
    }

    fn synthesize_dashscope_audio(
        &self,
        config: &Map<String, Value>,
        model: &str,
        text: &str,
        voice_prompt: &str,
    ) -> AppResult<Value> {
        let key = required(config, "api_key", "阿里云音频模型需要配置 API Key")?;
        let endpoint = config["endpoint"].as_str().filter(|value| !value.is_empty()).unwrap_or("https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation");
        let voice = config["voice"]
            .as_str()
            .filter(|value| !value.is_empty())
            .unwrap_or("Cherry");
        let mut input = json!({"text":text,"voice":voice,"language_type":"Chinese"});
        // Qwen3-TTS Instruct uses this field; Flash models safely retain their configured base voice.
        if model.to_lowercase().contains("instruct") {
            input["instruct"] = json!(voice_prompt);
        }
        self.client
            .post(endpoint)
            .header(AUTHORIZATION, format!("Bearer {key}"))
            .json(&json!({"model":model,"input":input}))
            .send()
            .map_err(|error| AppError::External(format!("阿里云音频请求失败：{error}")))?
            .error_for_status()
            .map_err(|error| AppError::External(format!("阿里云音频请求失败：{error}")))?
            .json::<Value>()
            .map_err(|error| AppError::External(format!("阿里云音频响应无效：{error}")))
    }

    fn synthesize_tencent_audio(
        &self,
        config: &Map<String, Value>,
        text: &str,
    ) -> AppResult<Value> {
        let voice = required(config, "voice", "腾讯云音频模型需要配置 VoiceId")?;
        self.tencent_request(
            config,
            "SyncDubbing",
            &json!({"Text":text,"VoiceId":voice,"Output":{"Type":"url"}}),
        )
    }
}

fn required<'a>(config: &'a Map<String, Value>, key: &str, message: &str) -> AppResult<&'a str> {
    config
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest(message.to_owned()))
}

fn audio_url(value: &Value) -> Option<&str> {
    value["audio_url"]
        .as_str()
        .or_else(|| value["output"]["audio"]["url"].as_str())
        .or_else(|| value["Response"]["AudioUrl"].as_str())
        .or_else(|| find_url(value))
}

fn audio_base64(value: &Value) -> Option<&str> {
    value["output"]["audio"]["data"]
        .as_str()
        .or_else(|| value["Response"]["AudioData"].as_str())
        .or_else(|| find_base64(value))
}
