//! Streaming language-provider adapter used by story-bible generation previews.

use std::io::{BufRead, BufReader};

use serde_json::{Map, Value};

use crate::error::{AppError, AppResult};

use super::{
    language_provider_error, language_request, language_response_read_error, model_for,
    provider_transport_error, response_text, ProviderClient,
};

impl ProviderClient {
    /// Stream one language completion so the long-drama flow can persist and display partial story-bible text.
    pub(crate) fn complete_with_web_search_stream(
        &self,
        kind: &str,
        selected_model: Option<&str>,
        system: &str,
        prompt: &str,
        enable_web_search: bool,
        on_delta: impl FnMut(&str) -> AppResult<()>,
    ) -> AppResult<Option<String>> {
        let config = self.config(kind)?;
        if !super::is_callable_text_config(&config, selected_model) {
            return Ok(None);
        }
        self.complete_config_with_web_search_stream(
            &config,
            selected_model,
            system,
            prompt,
            enable_web_search,
            true,
            on_delta,
        )
        .map(Some)
    }

    /// Stream only generated screenplay text, omitting provider reasoning from editable-script previews.
    pub(crate) fn complete_with_web_search_content_stream(
        &self,
        kind: &str,
        selected_model: Option<&str>,
        system: &str,
        prompt: &str,
        enable_web_search: bool,
        on_delta: impl FnMut(&str) -> AppResult<()>,
    ) -> AppResult<Option<String>> {
        let config = self.config(kind)?;
        if !super::is_callable_text_config(&config, selected_model) {
            return Ok(None);
        }
        self.complete_config_with_web_search_stream(
            &config,
            selected_model,
            system,
            prompt,
            enable_web_search,
            false,
            on_delta,
        )
        .map(Some)
    }

    fn complete_config_with_web_search_stream(
        &self,
        config: &Map<String, Value>,
        selected_model: Option<&str>,
        system: &str,
        prompt: &str,
        enable_web_search: bool,
        include_reasoning_preview: bool,
        mut on_delta: impl FnMut(&str) -> AppResult<()>,
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
        let payload = streaming_payload(payload);
        let response = self
            .client
            .post(url)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {key}"))
            .json(&payload)
            .send()
            .map_err(|error| provider_transport_error("语言模型流式", error))?;
        if !response.status().is_success() {
            return Err(AppError::External(language_provider_error(response)));
        }
        let is_event_stream = is_event_stream(
            response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
        );
        if is_event_stream {
            return stream_event_response(response, include_reasoning_preview, &mut on_delta);
        }
        let response = response
            .json::<Value>()
            .map_err(language_response_read_error)?;
        let text = response_text(&response)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::External("语言模型没有返回文本结果".to_owned()))?;
        on_delta(&text)?;
        Ok(text)
    }
}

fn is_event_stream(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|value| value.to_ascii_lowercase().contains("text/event-stream"))
}

fn streaming_payload(mut payload: Value) -> Value {
    payload["stream"] = Value::Bool(true);
    payload
}

fn stream_event_response(
    response: reqwest::blocking::Response,
    include_reasoning_preview: bool,
    on_delta: &mut impl FnMut(&str) -> AppResult<()>,
) -> AppResult<String> {
    let mut text = String::new();
    for line in BufReader::new(response).lines() {
        let line = line.map_err(stream_read_error)?;
        let Some(data) = line.strip_prefix("data:").map(str::trim) else {
            continue;
        };
        if data == "[DONE]" {
            break;
        }
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        if let Some(delta) = stream_delta(&event) {
            if include_reasoning_preview || delta.content.is_some() {
                on_delta(&delta.preview)?;
            }
            if let Some(content) = delta.content {
                text.push_str(&content);
            }
        } else if let Some(full) = response_text(&event) {
            if text.is_empty() {
                on_delta(&full)?;
                text.push_str(&full);
            }
        }
    }
    if text.trim().is_empty() {
        return Err(AppError::External("语言模型没有返回文本结果".to_owned()));
    }
    Ok(text)
}

struct StreamDelta {
    preview: String,
    content: Option<String>,
}

fn stream_delta(event: &Value) -> Option<StreamDelta> {
    let content = event["delta"]
        .as_str()
        .or_else(|| event["choices"][0]["delta"]["content"].as_str())
        .or_else(|| event["choices"][0]["text"].as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    if let Some(content) = content {
        return Some(StreamDelta {
            preview: content.clone(),
            content: Some(content),
        });
    }
    event["choices"][0]["delta"]["reasoning_content"]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(|reasoning| StreamDelta {
            preview: reasoning.to_owned(),
            content: None,
        })
}

fn stream_read_error(error: std::io::Error) -> AppError {
    AppError::External(format!("语言模型流式响应读取失败。原始错误：{error:?}"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{is_event_stream, stream_delta, streaming_payload};

    #[test]
    fn reads_openai_and_responses_api_stream_deltas() {
        assert_eq!(
            stream_delta(&json!({"choices":[{"delta":{"content":"分集"}}]}))
                .map(|delta| (delta.preview, delta.content)),
            Some(("分集".to_owned(), Some("分集".to_owned())))
        );
        assert_eq!(
            stream_delta(&json!({"type":"response.output_text.delta","delta":"大纲"}))
                .map(|delta| (delta.preview, delta.content)),
            Some(("大纲".to_owned(), Some("大纲".to_owned())))
        );
        assert_eq!(
            stream_delta(&json!({"choices":[{"delta":{"reasoning_content":"先规划"}}]}))
                .map(|delta| (delta.preview, delta.content)),
            Some(("先规划".to_owned(), None))
        );
    }

    #[test]
    fn routes_event_stream_content_to_the_incremental_reader() {
        assert!(is_event_stream(Some("text/event-stream; charset=utf-8")));
        assert!(is_event_stream(Some("Text/Event-Stream")));
        assert!(!is_event_stream(Some("application/json")));
    }

    #[test]
    fn marks_compatible_requests_for_streaming() {
        let payload = streaming_payload(json!({"model":"test-model"}));
        assert_eq!(payload["stream"], true);
    }
}
