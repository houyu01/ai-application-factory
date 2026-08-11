//! Minimal Rust-native adapters for configured OpenAI-compatible language and media providers.

use reqwest::header::AUTHORIZATION;
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    media::MediaStore,
    repository::Repository,
};

#[path = "providers_audio.rs"]
mod audio;
#[path = "providers_client.rs"]
mod client;
#[path = "providers_error_translation.rs"]
mod error_translation;
#[path = "providers_image.rs"]
mod image;
#[path = "providers_probe.rs"]
mod probe;
#[path = "providers_stream.rs"]
mod stream;
#[path = "providers_tencent.rs"]
mod tencent;
#[path = "providers_video.rs"]
mod video;
#[path = "providers_video_probe.rs"]
mod video_probe;

#[cfg(test)]
pub(crate) use client::MODEL_PROBE_TIMEOUT;
pub(crate) use error_translation::{
    provider_error_detail, translate_provider_error, translated_http_status,
};
#[cfg(test)]
pub(crate) use probe::ark_image_probe_is_reachable;

/// Provider result for a durable video task that may need later polling.
pub enum VideoJob {
    Ready(String),
    Pending {
        provider_task_id: String,
        progress: i64,
    },
}

/// Calls external model providers only from durable workers; project state itself never leaves local SQLite.
#[derive(Clone)]
pub struct ProviderClient {
    repository: Repository,
    media: MediaStore,
    client: reqwest::blocking::Client,
}

impl ProviderClient {
    /// Request structured text through a configured compatible endpoint; callers select a deterministic fallback if absent.
    pub fn complete(
        &self,
        kind: &str,
        selected_model: Option<&str>,
        system: &str,
        prompt: &str,
    ) -> AppResult<Option<String>> {
        let config = self.config(kind)?;
        if !is_callable_text_config(&config, selected_model) {
            return Ok(None);
        }
        self.complete_config(&config, selected_model, system, prompt)
            .map(Some)
    }

    /// Report whether a project-selected language model can be called without issuing a billable probe request.
    pub(crate) fn text_configured(
        &self,
        kind: &str,
        selected_model: Option<&str>,
    ) -> AppResult<bool> {
        Ok(is_callable_text_config(&self.config(kind)?, selected_model))
    }

    /// Run a drama-agent completion that can request Ark's built-in web search, matching Python's long-form flow.
    pub(crate) fn complete_with_web_search(
        &self,
        kind: &str,
        selected_model: Option<&str>,
        system: &str,
        prompt: &str,
        enable_web_search: bool,
    ) -> AppResult<Option<String>> {
        let config = self.config(kind)?;
        if !is_callable_text_config(&config, selected_model) {
            return Ok(None);
        }
        self.complete_config_with_web_search(
            &config,
            selected_model,
            system,
            prompt,
            enable_web_search,
        )
        .map(Some)
    }

    fn config(&self, kind: &str) -> AppResult<Map<String, Value>> {
        Ok(self
            .repository
            .setting(kind)?
            .as_object()
            .cloned()
            .unwrap_or_default())
    }

    pub(crate) fn complete_config(
        &self,
        config: &Map<String, Value>,
        selected_model: Option<&str>,
        system: &str,
        prompt: &str,
    ) -> AppResult<String> {
        self.complete_config_with_web_search(config, selected_model, system, prompt, false)
    }

    /// Perform one provider request with the same Ark-only built-in tool behavior as Python's compatible clients.
    pub(crate) fn complete_config_with_web_search(
        &self,
        config: &Map<String, Value>,
        selected_model: Option<&str>,
        system: &str,
        prompt: &str,
        enable_web_search: bool,
    ) -> AppResult<String> {
        let key = config
            .get("api_key")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let endpoint = config
            .get("endpoint")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let model = model_for(config, selected_model);
        if key.is_empty() || endpoint.is_empty() || model.is_empty() {
            return Err(AppError::BadRequest(
                "语言模型尚未配置 API Key、Endpoint 或模型名称".to_owned(),
            ));
        }
        let provider = config
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("ark");
        let (url, payload) = language_request(
            provider,
            endpoint,
            &model,
            system,
            prompt,
            enable_web_search,
        );
        let response = self
            .client
            .post(url)
            .header(AUTHORIZATION, format!("Bearer {key}"))
            .json(&payload)
            .send()
            .map_err(|error| provider_transport_error("语言模型", error))?;
        if !response.status().is_success() {
            return Err(AppError::External(language_provider_error(response)));
        }
        let response = response
            .json::<Value>()
            .map_err(language_response_read_error)?;
        response_text(&response)
            .filter(|text| !text.is_empty())
            .ok_or_else(|| AppError::External("语言模型没有返回文本结果".to_owned()))
    }
}

fn is_callable_text_config(config: &Map<String, Value>, selected_model: Option<&str>) -> bool {
    !config
        .get("api_key")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .is_empty()
        && !config
            .get("endpoint")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .is_empty()
        && !model_for(config, selected_model).is_empty()
}

pub(crate) fn response_text(response: &Value) -> Option<String> {
    response["choices"][0]["message"]["content"]
        .as_str()
        .or_else(|| response["output_text"].as_str())
        .map(str::to_owned)
        .or_else(|| {
            response["output"]
                .as_array()?
                .iter()
                .filter_map(|item| item["content"].as_array())
                .flatten()
                .find_map(|item| {
                    item["text"]
                        .as_str()
                        .or_else(|| item["text"]["value"].as_str())
                })
                .map(str::to_owned)
        })
}

fn join_endpoint(endpoint: &str, path: &str) -> String {
    format!("{}/{}", endpoint.trim_end_matches('/'), path)
}

/// Preserve the image endpoint selected in Settings, accepting either a base URL or the full generation URL.
pub(crate) fn image_generation_endpoint(endpoint: &str) -> String {
    let endpoint = endpoint.trim().trim_end_matches('/');
    if endpoint.ends_with("/images/generations") {
        endpoint.to_owned()
    } else {
        join_endpoint(endpoint, "images/generations")
    }
}

/// Select Ark's request dialect from the configured base URL so saved Plan endpoints are never sent to Responses API.
pub(crate) fn language_request(
    provider: &str,
    endpoint: &str,
    model: &str,
    system: &str,
    prompt: &str,
    enable_web_search: bool,
) -> (String, Value) {
    if provider == "ark" && !endpoint.contains("/api/plan/") {
        let mut request = json!({"model":model,"input":[{"role":"system","content":system},{"role":"user","content":prompt}]});
        if enable_web_search {
            request["tools"] = json!([{"type":"web_search"}]);
        }
        (responses_endpoint(endpoint), request)
    } else {
        (
            chat_completions_endpoint(endpoint),
            json!({"model":model,"messages":[{"role":"system","content":system},{"role":"user","content":prompt}]}),
        )
    }
}

fn responses_endpoint(endpoint: &str) -> String {
    let endpoint = endpoint.trim_end_matches('/');
    if endpoint.ends_with("/responses") {
        endpoint.to_owned()
    } else {
        join_endpoint(endpoint, "responses")
    }
}

fn chat_completions_endpoint(endpoint: &str) -> String {
    let endpoint = endpoint.trim_end_matches('/');
    if endpoint.ends_with("/chat/completions") {
        endpoint.to_owned()
    } else if endpoint.ends_with("/chat") {
        join_endpoint(endpoint, "completions")
    } else {
        join_endpoint(endpoint, "chat/completions")
    }
}

/// Preserve a provider's structured error code and message so the settings UI can distinguish credentials from model compatibility.
fn language_provider_error(response: reqwest::blocking::Response) -> String {
    let status = response.status();
    let detail = response
        .text()
        .ok()
        .as_deref()
        .and_then(provider_error_detail)
        .or_else(|| translated_http_status(status.as_u16()).map(str::to_owned))
        .unwrap_or_else(|| "服务商未返回可识别的错误详情，请检查模型配置后重试。".to_owned());
    format!("语言模型请求失败：{detail}")
}

/// Keep a direct provider response-read error intact for the UI and durable task record.
pub(super) fn language_response_read_error(_error: impl std::fmt::Display) -> AppError {
    AppError::External("语言模型返回的内容格式无效，请检查服务地址和模型配置后重试。".to_owned())
}

/// Convert transport failures into Chinese guidance without leaking raw HTTP-client diagnostics to the creator.
pub(super) fn provider_transport_error(label: &str, error: reqwest::Error) -> AppError {
    let category = error
        .status()
        .and_then(|status| translated_http_status(status.as_u16()))
        .unwrap_or_else(|| {
            if error.is_timeout() {
                "请求超时"
            } else if error.is_connect() {
                "连接失败"
            } else if error.is_body() {
                "请求或响应体传输失败"
            } else if error.is_decode() {
                "响应解码失败"
            } else {
                "请求发送失败"
            }
        });
    AppError::External(format!("{label}请求失败：{category}"))
}

pub(crate) fn image_size(ratio: &str) -> &str {
    match ratio {
        "16:9" => "1536x1024",
        "1:1" => "1024x1024",
        "3:4" => "1024x1536",
        "4:3" => "1536x1024",
        _ => "1024x1536",
    }
}
fn model_for(config: &Map<String, Value>, selected: Option<&str>) -> String {
    let configured = config
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    selected
        .filter(|model| !model.is_empty() && (configured.is_empty() || configured.contains(model)))
        .or_else(|| config.get("model").and_then(Value::as_str))
        .unwrap_or_default()
        .to_owned()
}
pub(crate) fn image_prompt(prompt: &str) -> String {
    let tag = "标识要求（必须遵守，优先级最高）：在画面左上角添加“AI生成”标签。标签使用小型圆角矩形、深色半透明底、细浅色描边与浅灰文字，距上边和左边保留安全边距，不遮挡主体；除该标签外不添加其他文字或水印。";
    [prompt.trim(), tag]
        .into_iter()
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}
fn find_url(value: &Value) -> Option<&str> {
    value["data"][0]["url"]
        .as_str()
        .or_else(|| value["output"]["results"][0]["url"].as_str())
        .or_else(|| {
            value["output"]["choices"][0]["message"]["content"]
                .as_array()
                .and_then(|items| {
                    items
                        .iter()
                        .find_map(|item| item["image"].as_str().or_else(|| item["url"].as_str()))
                })
        })
        .or_else(|| value["output"]["video_url"].as_str())
        .or_else(|| value["url"].as_str())
}
fn find_base64(value: &Value) -> Option<&str> {
    value["data"][0]["b64_json"]
        .as_str()
        .or_else(|| value["output"]["results"][0]["b64_json"].as_str())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use crate::{db::Database, media::MediaStore, repository::Repository, value::new_id};

    use super::ProviderClient;

    #[test]
    fn video_provider_config_never_inherits_image_endpoints_or_credentials() {
        let root = std::env::temp_dir().join(format!("isolated-video-config-{}", new_id()));
        let repository =
            Repository::new(Database::open(root.join("settings.db")).expect("database"));
        repository
            .set_setting(
                "multimodal",
                json!({"endpoint":"https://image.example/generate","api_key":"image-key"}),
            )
            .expect("image config");
        repository
            .set_setting(
                "video",
                json!({"create_url":"https://video.example/create","api_key":"video-key"}),
            )
            .expect("video config");
        let client = ProviderClient::new(
            repository.clone(),
            MediaStore::new(repository).expect("media store"),
        )
        .expect("provider client");
        let config = client.config("video").expect("video config");
        assert_eq!(config["create_url"], "https://video.example/create");
        assert_eq!(config["api_key"], "video-key");
        assert!(config.get("endpoint").is_none());
        fs::remove_dir_all(root).expect("remove test data");
    }
}
