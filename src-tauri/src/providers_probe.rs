//! Real provider probes performed before model settings are committed to SQLite.
use super::video_probe::ark_video_probe_is_reachable;
use crate::{
    error::{AppError, AppResult},
    providers::{
        find_base64, find_url, image_generation_endpoint, model_for, provider_transport_error,
        ProviderClient, VideoJob,
    },
};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Map, Value};

impl ProviderClient {
    /// Send the smallest real request for a candidate model configuration before it can replace a working one.
    pub(crate) fn probe_model_config(&self, config: &Map<String, Value>) -> AppResult<()> {
        let kind = config
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let model = model_for(config, None);
        let label = model_kind_label(kind);
        validate_probe_config(config, kind, &model)?;
        let result = match kind {
            "language" => self
                .complete_config(config, Some(&model), "", "请只回复：OK")
                .map(|_| ()),
            "multimodal" => self.probe_image(config, &model),
            "video" => self.probe_video(config, &model),
            "audio" => self.probe_audio(config, &model),
            _ => Err(AppError::BadRequest(format!(
                "不支持嗅探的模型类型：{kind}"
            ))),
        };
        result.map_err(|error| {
            AppError::BadRequest(format!(
                "{label}模型嗅探失败（实际调用模型：{model}）：{error}"
            ))
        })
    }

    fn probe_image(&self, config: &Map<String, Value>, model: &str) -> AppResult<()> {
        let provider = provider(config);
        if provider == "tencent" {
            return self.probe_tencent_image_credentials(config);
        }
        let key = api_key(config, "图像")?;
        let prompt = format!(
            "生成一张简单的纯色测试图\n\n{}",
            AI_GENERATED_IMAGE_TAG_PROMPT
        );
        let (url, payload) = if provider == "dashscope" {
            (
                endpoint_or(config, "endpoint", "https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation"),
                json!({"model":model,"input":{"messages":[{"role":"user","content":[{"text":prompt}]}]},"parameters":{"size":"1024*1024","n":1}}),
            )
        } else {
            (
                image_generation_endpoint(&endpoint_or(config, "endpoint", "")),
                json!({"model":model,"prompt":prompt,"size":"2K","sequential_image_generation":"disabled","response_format":"url","watermark":false}),
            )
        };
        if url.is_empty() {
            return Err(AppError::BadRequest("图像模型未配置 Endpoint".to_owned()));
        }
        let response = self
            .client
            .post(url)
            .header(AUTHORIZATION, format!("Bearer {key}"))
            .header(CONTENT_TYPE, "application/json")
            .json(&payload)
            .send()
            .map_err(|error| provider_transport_error("图片模型", error))?;
        if provider == "ark" && ark_image_probe_is_reachable(response.status()) {
            return Ok(());
        }
        if !response.status().is_success() {
            return Err(super::image::image_provider_error(response));
        }
        let response = response
            .json::<Value>()
            .map_err(|error| AppError::External(format!("图片模型响应无效：{error}")))?;
        if find_url(&response).is_some() || find_base64(&response).is_some() {
            Ok(())
        } else {
            Err(AppError::External("图片模型没有返回有效结果".to_owned()))
        }
    }

    fn probe_video(&self, config: &Map<String, Value>, model: &str) -> AppResult<()> {
        let provider = provider(config);
        if provider == "tencent" {
            return self.probe_tencent_video_credentials(config);
        }
        let references = dashscope_probe_references(config, model);
        let created = match self.start_video_with_config(
            config,
            "生成一个简单的测试视频：静态风景，镜头缓慢推进。",
            "16:9",
            if provider == "dashscope" {
                "720p"
            } else {
                "480p"
            },
            3,
            &references,
            &[],
            None,
            Some(model),
        ) {
            Ok(created) => created,
            Err(error) if provider == "ark" && ark_video_probe_is_reachable(&error) => {
                return Ok(())
            }
            Err(error) => return Err(error),
        };
        if provider == "ark" {
            if let VideoJob::Pending {
                provider_task_id, ..
            } = created
            {
                let _ = self.cancel_video_with_config(config, &provider_task_id);
            }
            return Ok(());
        }
        let VideoJob::Pending {
            provider_task_id, ..
        } = created
        else {
            return Ok(());
        };
        let outcome = self
            .poll_video_with_config(config, &provider_task_id)
            .map(|_| ());
        let _ = self.cancel_video_with_config(config, &provider_task_id);
        outcome
    }

    fn probe_audio(&self, config: &Map<String, Value>, model: &str) -> AppResult<()> {
        match provider(config) {
            "dashscope" => self.probe_dashscope_audio(config, model),
            "tencent" => self.probe_tencent_audio(config),
            _ => self.probe_ark_audio(config),
        }
    }

    fn probe_ark_audio(&self, config: &Map<String, Value>) -> AppResult<()> {
        self.synthesize_ark_audio_bytes(
            config,
            "模型连接测试",
            crate::volcengine_tts::DEFAULT_FEMALE_SPEAKER,
            "这是模型连接测试，请使用自然、清晰的中文女声。",
        )?;
        self.synthesize_ark_audio_bytes(
            config,
            "模型连接测试",
            crate::volcengine_tts::DEFAULT_MALE_SPEAKER,
            "这是模型连接测试，请使用自然、清晰的中文男声。",
        )?;
        Ok(())
    }

    fn probe_dashscope_audio(&self, config: &Map<String, Value>, model: &str) -> AppResult<()> {
        let key = api_key(config, "音频")?;
        let voice = config
            .get("voice")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or("Cherry");
        let response = self
            .client
            .post(endpoint_or(config, "endpoint", "https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation"))
            .header(AUTHORIZATION, format!("Bearer {key}"))
            .json(&json!({"model":model,"input":{"text":"模型连接测试","voice":voice,"language_type":"Chinese"}}))
            .send()
            .map_err(|error| AppError::External(format!("阿里云音频请求失败：{error}")))?
            .error_for_status()
            .map_err(|error| AppError::External(format!("阿里云音频请求失败：{error}")))?
            .json::<Value>()
            .map_err(|error| AppError::External(format!("阿里云音频响应无效：{error}")))?;
        if response["output"]["audio"]["url"]
            .as_str()
            .is_some_and(|url| !url.is_empty())
            || response["output"]["audio"]["data"]
                .as_str()
                .is_some_and(|data| !data.is_empty())
        {
            Ok(())
        } else {
            Err(AppError::External(
                "阿里云音频模型没有返回音频结果".to_owned(),
            ))
        }
    }

    fn probe_tencent_audio(&self, config: &Map<String, Value>) -> AppResult<()> {
        let voice = required(config, "voice", "腾讯云音频模型需要配置 VoiceId")?;
        let response = self.tencent_request(
            config,
            "SyncDubbing",
            &json!({"Text":"模型连接测试","VoiceId":voice,"Output":{"Type":"url"}}),
        )?;
        let output = &response["Response"];
        if output["AudioData"]
            .as_str()
            .is_some_and(|data| !data.is_empty())
            || output["AudioUrl"]
                .as_str()
                .is_some_and(|url| !url.is_empty())
        {
            return Ok(());
        }
        Err(AppError::External(
            "腾讯云音频模型没有返回音频结果".to_owned(),
        ))
    }
}

/// Treat Ark responses that prove a request reached the configured model service as a successful settings probe.
pub(crate) fn ark_image_probe_is_reachable(status: reqwest::StatusCode) -> bool {
    status.is_success() || matches!(status.as_u16(), 400 | 409 | 422 | 429)
}

const AI_GENERATED_IMAGE_TAG_PROMPT: &str = "标识要求（必须遵守，优先级最高）：在画面左上角添加“AI生成”标签。标签使用小型圆角矩形、深色半透明底、细浅色描边与浅灰文字，距上边和左边保留安全边距，不遮挡主体；除该标签外不添加其他文字或水印。";

fn validate_probe_config(config: &Map<String, Value>, kind: &str, model: &str) -> AppResult<()> {
    let provider = provider(config);
    if provider == "tencent" && matches!(kind, "audio" | "multimodal" | "video") {
        required(
            config,
            "secret_id",
            &format!("腾讯云{}模型未配置 SecretId", model_kind_label(kind)),
        )?;
        required(
            config,
            "secret_key",
            &format!("腾讯云{}模型未配置 SecretKey", model_kind_label(kind)),
        )?;
    } else {
        api_key(config, model_kind_label(kind))?;
    }
    if model.is_empty() {
        return Err(AppError::BadRequest(format!(
            "{}模型未配置模型名称",
            model_kind_label(kind)
        )));
    }
    Ok(())
}

fn dashscope_probe_references(config: &Map<String, Value>, model: &str) -> Vec<String> {
    let normal = model.to_lowercase();
    if provider(config) == "dashscope"
        && ((normal.contains("happyhorse") && normal.contains("-r2v"))
            || normal.starts_with("wan2.7-r2v"))
    {
        vec!["https://cdn.translate.alibaba.com/r/wanx-demo-1.png".to_owned()]
    } else {
        Vec::new()
    }
}

fn provider(config: &Map<String, Value>) -> &str {
    config
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("ark")
}
fn api_key<'a>(config: &'a Map<String, Value>, label: &str) -> AppResult<&'a str> {
    required(config, "api_key", &format!("{label}模型未配置 API Key"))
}
fn required<'a>(config: &'a Map<String, Value>, key: &str, message: &str) -> AppResult<&'a str> {
    config
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest(message.to_owned()))
}
fn endpoint_or(config: &Map<String, Value>, key: &str, fallback: &str) -> String {
    config
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_owned()
}
fn model_kind_label(kind: &str) -> &str {
    match kind {
        "language" => "语言",
        "multimodal" => "图像",
        "video" => "视频",
        "audio" => "音频",
        _ => kind,
    }
}
