//! Converts provider-facing failures into concise, actionable Chinese messages for durable tasks.

use serde_json::Value;

/// Read a structured provider error response and return its Chinese user-facing explanation.
pub(crate) fn provider_error_detail(body: &str) -> Option<String> {
    let response = serde_json::from_str::<Value>(body).ok()?;
    let error = response
        .get("error")
        .filter(|value| value.is_object())
        .unwrap_or(&response);
    let code = error
        .get("code")
        .and_then(Value::as_str)
        .or_else(|| response.get("code").and_then(Value::as_str))
        .unwrap_or_default();
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| response.get("message").and_then(Value::as_str))?
        .trim();
    (!message.is_empty()).then(|| translate_provider_error(code, message))
}

/// Translate a provider code or message without exposing the raw upstream payload to the creator.
pub(crate) fn translate_provider_error(code: &str, message: &str) -> String {
    let code = code.trim();
    let message = message.trim();
    known_code(code)
        .or_else(|| known_message(message))
        .map(str::to_owned)
        .or_else(|| contains_chinese(message).then(|| message.to_owned()))
        .unwrap_or_else(|| {
            if code.is_empty() {
                "服务商未能完成请求，请检查模型配置、提示词和参考素材后重试。".to_owned()
            } else {
                format!(
                    "服务商返回错误（错误码：{code}）。请检查模型配置、提示词和参考素材后重试。"
                )
            }
        })
}

/// Map HTTP status codes to an actionable message when the provider omits an error body.
pub(crate) fn translated_http_status(status: u16) -> Option<&'static str> {
    match status {
        400 => Some("请求参数不符合服务商要求，请检查模型、提示词和参考素材后重试。"),
        401 => Some("API Key 无效或已失效，请检查模型配置中的 API Key。"),
        403 => Some("当前账号没有调用此模型的权限，请检查模型开通状态和 API Key 权限。"),
        404 => Some("所选模型或服务地址不存在，请检查 Endpoint 和模型名称。"),
        408 => Some("服务商响应超时，请稍后重试。"),
        409 => Some("请求与当前任务状态冲突，请稍后重试。"),
        413 => Some("提交的图片或请求内容过大，请缩小素材后重试。"),
        415 => Some("服务商不支持当前素材格式，请更换图片或视频格式后重试。"),
        422 => Some("提交内容未通过服务商校验，请修改提示词或参考素材后重试。"),
        429 => Some("请求过于频繁或账户额度不足，请稍后重试并检查账户额度。"),
        500..=599 => Some("服务商暂时不可用，请稍后重试。"),
        _ => None,
    }
}

fn known_code(code: &str) -> Option<&'static str> {
    let code = code.to_ascii_lowercase();
    if code.contains("inputimagesensitivecontentdetected") || code.contains("privacyinformation") {
        Some("检测到输入图片可能包含真人或个人隐私信息，服务商拒绝生成。请替换为不含真人或隐私信息的图片后重试。")
    } else if code.contains("unsupportedmodel") || code.contains("modelnotsupport") {
        Some("当前模型不支持此功能，请在设置中更换支持该功能的模型后重试。")
    } else if code.contains("invalidapikey")
        || code.contains("authentication")
        || code.contains("unauthorized")
    {
        Some("API Key 无效或已失效，请检查模型配置中的 API Key。")
    } else if code.contains("permission")
        || code.contains("forbidden")
        || code.contains("accessdenied")
    {
        Some("当前账号没有调用此模型的权限，请检查模型开通状态和 API Key 权限。")
    } else if code.contains("ratelimit")
        || code.contains("quota")
        || code.contains("toomanyrequest")
    {
        Some("请求过于频繁或账户额度不足，请稍后重试并检查账户额度。")
    } else if code.contains("sensitive")
        || code.contains("contentpolicy")
        || code.contains("safety")
    {
        Some("输入内容未通过安全审核，请修改提示词或参考素材后重试。")
    } else if code.contains("invalidparameter")
        || code.contains("badrequest")
        || code.contains("validation")
    {
        Some("请求参数不符合服务商要求，请检查模型、提示词和参考素材后重试。")
    } else if code.contains("notfound") || code.contains("resourcenotfound") {
        Some("所选模型或资源不存在，请检查模型名称和服务配置。")
    } else {
        None
    }
}

fn known_message(message: &str) -> Option<&'static str> {
    let message = message.to_ascii_lowercase();
    if message.contains("real person") || message.contains("privacy information") {
        Some("检测到输入图片可能包含真人或个人隐私信息，服务商拒绝生成。请替换为不含真人或隐私信息的图片后重试。")
    } else if message.contains("does not support") || message.contains("unsupported model") {
        Some("当前模型不支持此功能，请在设置中更换支持该功能的模型后重试。")
    } else if message.contains("invalid api key")
        || message.contains("incorrect api key")
        || message.contains("unauthorized")
    {
        Some("API Key 无效或已失效，请检查模型配置中的 API Key。")
    } else if message.contains("forbidden") || message.contains("permission denied") {
        Some("当前账号没有调用此模型的权限，请检查模型开通状态和 API Key 权限。")
    } else if message.contains("rate limit")
        || message.contains("quota")
        || message.contains("too many requests")
    {
        Some("请求过于频繁或账户额度不足，请稍后重试并检查账户额度。")
    } else if message.contains("sensitive content")
        || message.contains("content policy")
        || message.contains("safety policy")
    {
        Some("输入内容未通过安全审核，请修改提示词或参考素材后重试。")
    } else if message.contains("invalid parameter") || message.contains("bad request") {
        Some("请求参数不符合服务商要求，请检查模型、提示词和参考素材后重试。")
    } else if message.contains("not found") {
        Some("所选模型或资源不存在，请检查模型名称和服务配置。")
    } else if message.contains("timeout") || message.contains("timed out") {
        Some("服务商响应超时，请稍后重试。")
    } else if message.contains("service unavailable") || message.contains("internal server error") {
        Some("服务商暂时不可用，请稍后重试。")
    } else {
        http_status_in(&message).and_then(translated_http_status)
    }
}

fn http_status_in(message: &str) -> Option<u16> {
    [
        400, 401, 403, 404, 408, 409, 413, 415, 422, 429, 500, 502, 503, 504,
    ]
    .into_iter()
    .find(|status| message.contains(&status.to_string()))
}

fn contains_chinese(message: &str) -> bool {
    message
        .chars()
        .any(|character| ('\u{4e00}'..='\u{9fff}').contains(&character))
}

#[cfg(test)]
mod tests {
    use super::{provider_error_detail, translate_provider_error, translated_http_status};

    #[test]
    fn translates_ark_reference_image_privacy_errors() {
        let detail = provider_error_detail(
            r#"{"error":{"code":"InputImageSensitiveContentDetected.PrivacyInformation","message":"The request failed because the input image may contain a real person."}}"#,
        );

        assert_eq!(
            detail.as_deref(),
            Some("检测到输入图片可能包含真人或个人隐私信息，服务商拒绝生成。请替换为不含真人或隐私信息的图片后重试。")
        );
    }

    #[test]
    fn hides_unknown_english_provider_messages() {
        let detail = translate_provider_error("UnknownProviderCode", "upstream-only detail");

        assert!(detail.contains("错误码：UnknownProviderCode"));
        assert!(!detail.contains("upstream-only detail"));
    }

    #[test]
    fn maps_missing_response_bodies_from_status() {
        assert_eq!(
            translated_http_status(429),
            Some("请求过于频繁或账户额度不足，请稍后重试并检查账户额度。")
        );
    }
}
