//! Shared video provider response parsing and request helpers.

use reqwest::blocking::Response;
use serde_json::{Map, Value};

use crate::{
    error::{AppError, AppResult},
    providers::{
        find_url, provider_error_detail, translate_provider_error, translated_http_status,
    },
};

pub(super) fn api_key(config: &Map<String, Value>) -> AppResult<&str> {
    config
        .get("api_key")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest("视频模型尚未配置 API Key".to_owned()))
}

pub(super) fn unique(values: &[String]) -> Vec<String> {
    values
        .iter()
        .filter(|item| !item.is_empty())
        .fold(Vec::new(), |mut all, item| {
            if !all.contains(item) {
                all.push(item.clone());
            }
            all
        })
}

pub(super) fn task_url(template: &str, id: &str) -> String {
    if template.contains("{id}") || template.contains("{task_id}") {
        template.replace("{id}", id).replace("{task_id}", id)
    } else {
        format!("{}/{}", template.trim_end_matches('/'), id)
    }
}

fn nested(value: &Value) -> &Value {
    value
        .get("output")
        .or_else(|| {
            value
                .get("data")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
        })
        .or_else(|| value.get("data"))
        .or_else(|| value.get("Response"))
        .unwrap_or(value)
}

pub(super) fn task_id(value: &Value) -> Option<String> {
    [value, nested(value)]
        .iter()
        .find_map(|item| {
            item["task_id"]
                .as_str()
                .or_else(|| item["TaskId"].as_str())
                .or_else(|| item["id"].as_str())
        })
        .map(str::to_owned)
}

pub(super) fn task_status(value: &Value) -> String {
    let raw = [value, nested(value)]
        .iter()
        .find_map(|item| {
            item["task_status"]
                .as_str()
                .or_else(|| item["status"].as_str())
                .or_else(|| item["Status"].as_str())
        })
        .unwrap_or_default()
        .to_lowercase();
    match raw.as_str() {
        "wait" | "pending" => "queued".to_owned(),
        "run" => "running".to_owned(),
        other => other.to_owned(),
    }
}

pub(super) fn progress(value: &Value) -> i64 {
    [value, nested(value)]
        .iter()
        .find_map(|item| item["progress"].as_i64())
        .unwrap_or_else(|| match task_status(value).as_str() {
            "queued" => 5,
            "running" => 50,
            "succeeded" | "done" => 100,
            _ => 0,
        })
}

pub(super) fn video_url(value: &Value) -> Option<&str> {
    find_url(value)
        .or_else(|| response_url(&value["video_url"]))
        .or_else(|| response_url(&value["content"]["video_url"]))
        .or_else(|| response_url(&value["content"]["url"]))
        .or_else(|| {
            value["content"].as_array().and_then(|items| {
                items.iter().find_map(|item| {
                    response_url(&item["video_url"]).or_else(|| response_url(&item["url"]))
                })
            })
        })
        .or_else(|| response_url(&nested(value)["video_url"]))
        .or_else(|| response_url(&nested(value)["url"]))
        .or_else(|| {
            nested(value)["VideoUrls"]
                .as_array()
                .and_then(|items| items.first())
                .and_then(response_url)
        })
}

fn response_url(value: &Value) -> Option<&str> {
    value.as_str().or_else(|| value["url"].as_str())
}

pub(super) fn task_error(value: &Value) -> Option<String> {
    [value, nested(value)].iter().find_map(|item| {
        let error = item
            .get("error")
            .filter(|error| error.is_object())
            .unwrap_or(item);
        let code = error["code"]
            .as_str()
            .or_else(|| item["code"].as_str())
            .unwrap_or_default();
        error["error_message"]
            .as_str()
            .or_else(|| error["message"].as_str())
            .or_else(|| item["error_message"].as_str())
            .or_else(|| item["message"].as_str())
            .or_else(|| item["Message"].as_str())
            .map(|message| translate_provider_error(code, message))
    })
}

/// Read a video-provider response once so failures retain the exact body returned by the upstream service.
pub(super) fn video_json_response(provider: &str, response: Response) -> AppResult<Value> {
    let status = response.status();
    let body = response
        .text()
        .map_err(|_| AppError::External(format!("{provider} 视频响应读取失败，请稍后重试。")))?;
    if !status.is_success() {
        return Err(video_response_error(
            provider,
            &format!("HTTP {status}"),
            &body,
        ));
    }
    serde_json::from_str(&body).map_err(|error| {
        video_response_error(provider, &format!("响应不是有效 JSON：{error}"), &body)
    })
}

/// Attach and log the original provider response for a terminal video task failure.
pub(super) fn video_task_response_error(
    provider: &str,
    message: &str,
    response: &Value,
) -> AppError {
    let raw_response = serde_json::to_string(response)
        .unwrap_or_else(|error| format!("无法序列化原始响应：{error}"));
    video_response_error(provider, message, &raw_response)
}

fn video_response_error(provider: &str, message: &str, raw_response: &str) -> AppError {
    let raw_response = if raw_response.trim().is_empty() {
        "（服务未返回响应体）"
    } else {
        raw_response
    };
    eprintln!("{provider} 视频服务失败：{message}\n原始响应：{raw_response}");
    let request_detail = translated_http_status_from(message)
        .map(str::to_owned)
        .unwrap_or_else(|| translate_provider_error("", message));
    let provider_detail = provider_error_detail(raw_response);
    let detail = provider_detail
        .filter(|detail| detail != &request_detail)
        .map(|detail| format!("{request_detail}：{detail}"))
        .unwrap_or(request_detail);
    AppError::External(format!("{provider} 视频服务失败：{detail}"))
}

pub(super) fn video_request_error(provider: &str) -> impl Fn(reqwest::Error) -> AppError + '_ {
    move |error| {
        let detail = error
            .status()
            .and_then(|status| translated_http_status(status.as_u16()))
            .unwrap_or_else(|| {
                if error.is_timeout() {
                    "请求超时，请稍后重试。"
                } else if error.is_connect() {
                    "无法连接到服务商，请检查网络和服务地址。"
                } else {
                    "请求发送失败，请检查网络和模型配置后重试。"
                }
            });
        AppError::External(format!("{provider} 视频请求失败：{detail}"))
    }
}

fn translated_http_status_from(message: &str) -> Option<&'static str> {
    [
        400, 401, 403, 404, 408, 409, 413, 415, 422, 429, 500, 502, 503, 504,
    ]
    .into_iter()
    .find(|status| message.contains(&status.to_string()))
    .and_then(translated_http_status)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{video_task_response_error, video_url};

    #[test]
    fn ark_plan_success_response_reads_content_video_url() {
        let response = json!({
            "id": "cgt-20260805135906-vkqzn",
            "status": "succeeded",
            "content": {"video_url": "https://cdn.example/generated.mp4"},
        });

        assert_eq!(
            video_url(&response),
            Some("https://cdn.example/generated.mp4")
        );
    }

    #[test]
    fn terminal_video_failure_keeps_the_complete_provider_response() {
        let error = video_task_response_error(
            "Ark",
            "视频任务已完成，但没有返回 video_url",
            &json!({"task_status":"succeeded","output":{"message":"provider detail"}}),
        );

        let message = error.to_string();
        assert!(message.contains("视频任务已完成，但没有返回 video_url"));
        assert!(message.contains("原始响应："));
        assert!(message.contains("\"task_status\":\"succeeded\""));
        assert!(message.contains("provider detail"));
    }
}
