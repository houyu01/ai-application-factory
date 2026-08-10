//! Native Ark, DashScope, and Tencent MPS image-generation protocols.

use std::{thread, time::Duration};

use base64::Engine;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    providers::{
        find_base64, find_url, image_generation_endpoint, image_prompt, image_size, model_for,
        provider_error_detail, provider_transport_error, tencent::tencent_model_parts,
        translated_http_status, ProviderClient,
    },
};

impl ProviderClient {
    /// Generate one image through the provider-native payload and persist the returned URL or bytes locally.
    pub fn image(
        &self,
        prompt: &str,
        ratio: &str,
        references: &[String],
        selected_model: Option<&str>,
    ) -> AppResult<String> {
        let config = self.config("multimodal")?;
        let model = model_for(&config, selected_model);
        if model.is_empty() {
            return Err(AppError::BadRequest("图像模型尚未配置模型名称".to_owned()));
        }
        let prompt = image_prompt(prompt);
        let provider = config["provider"].as_str().unwrap_or("ark");
        if provider == "tencent" {
            return self.tencent_mps_image(&config, &model, &prompt, references);
        }
        let key = config["api_key"].as_str().unwrap_or_default();
        let endpoint = config["endpoint"].as_str().unwrap_or_default();
        if key.is_empty() || endpoint.is_empty() {
            return Err(AppError::BadRequest(
                "图像模型尚未配置 API Key 或 Endpoint".to_owned(),
            ));
        }
        let response = match provider {
            "dashscope" => {
                self.dashscope_image(key, endpoint, &model, &prompt, ratio, references)?
            }
            _ => self.ark_image(key, endpoint, &model, &prompt, references)?,
        };
        if let Some(url) = find_url(&response) {
            return self.media.save_url(url, ".png");
        }
        if let Some(encoded) = find_base64(&response) {
            return self.media.save(
                &base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|_| AppError::External("图片模型返回了无效 base64".to_owned()))?,
                ".png",
                "image/png",
            );
        }
        Err(AppError::External(
            "图片模型没有返回图片 URL 或 b64_json".to_owned(),
        ))
    }

    fn ark_image(
        &self,
        key: &str,
        endpoint: &str,
        model: &str,
        prompt: &str,
        references: &[String],
    ) -> AppResult<Value> {
        let mut payload = json!({"model":model,"prompt":prompt,"size":"2K","sequential_image_generation":"disabled","response_format":"url","watermark":false});
        if !references.is_empty() {
            payload["image"] = json!(references);
        }
        self.image_request(image_generation_endpoint(endpoint), key, &payload)
    }

    fn dashscope_image(
        &self,
        key: &str,
        endpoint: &str,
        model: &str,
        prompt: &str,
        ratio: &str,
        references: &[String],
    ) -> AppResult<Value> {
        let mut content = vec![json!({"text":prompt})];
        content.extend(
            references
                .iter()
                .filter(|value| !value.is_empty())
                .map(|value| json!({"image":value})),
        );
        self.image_request(endpoint, key, &json!({"model":model,"input":{"messages":[{"role":"user","content":content}]},"parameters":{"size":image_size(ratio).replace('x',"*"),"n":1}}))
    }

    /// Create and poll a Tencent MPS image task, copying its short-lived result URL into local media.
    fn tencent_mps_image(
        &self,
        config: &Map<String, Value>,
        model: &str,
        prompt: &str,
        references: &[String],
    ) -> AppResult<String> {
        let payload = tencent_image_payload(model, prompt, references)?;
        let created = self.tencent_request(config, "CreateAigcImageTask", &payload)?;
        let id = created["Response"]["TaskId"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::External("腾讯云 MPS 图像模型没有返回任务 ID".to_owned()))?
            .to_owned();
        for _ in 0..90 {
            let result =
                self.tencent_request(config, "DescribeAigcImageTask", &json!({"TaskId":id}))?;
            match tencent_image_status(&result).as_str() {
                "done" => {
                    let url = tencent_image_url(&result).ok_or_else(|| {
                        AppError::External(
                            "腾讯云 MPS 图像任务已完成，但没有返回 ImageUrls".to_owned(),
                        )
                    })?;
                    return self.media.save_url(url, ".png");
                }
                "fail" | "failed" | "error" | "cancelled" => {
                    return Err(AppError::External(format!(
                        "腾讯云 MPS 图像任务失败：{}",
                        result["Response"]["Message"].as_str().unwrap_or("未知错误")
                    )))
                }
                _ => thread::sleep(Duration::from_secs(2)),
            }
        }
        Err(AppError::External("腾讯云 MPS 图像任务超时".to_owned()))
    }

    fn image_request(
        &self,
        url: impl reqwest::IntoUrl,
        key: &str,
        payload: &Value,
    ) -> AppResult<Value> {
        let response = self
            .client
            .post(url)
            .header(AUTHORIZATION, format!("Bearer {key}"))
            .header(CONTENT_TYPE, "application/json")
            .json(payload)
            .send()
            .map_err(|error| provider_transport_error("图片模型", error))?;
        if !response.status().is_success() {
            return Err(image_provider_error(response));
        }
        response.json::<Value>().map_err(|_| {
            AppError::External(
                "图片模型返回的内容格式无效，请检查服务地址和模型配置后重试。".to_owned(),
            )
        })
    }
}

fn tencent_image_payload(model: &str, prompt: &str, references: &[String]) -> AppResult<Value> {
    if references.iter().any(|value| !value.is_empty()) {
        return Err(AppError::BadRequest(format!(
            "腾讯云 MPS 模型“{model}”在当前工作台仅支持文生图；请移除参考图，或改用支持图像编辑的服务商模型。"
        )));
    }
    let (name, version) = tencent_model_parts(model, "Hunyuan", "3.0");
    Ok(json!({
        "ModelName": name,
        "ModelVersion": version,
        "Prompt": prompt.chars().take(1000).collect::<String>(),
        "EnhancePrompt": true,
    }))
}

fn tencent_image_status(response: &Value) -> String {
    response["Response"]["Status"]
        .as_str()
        .unwrap_or_default()
        .to_lowercase()
}

fn tencent_image_url(response: &Value) -> Option<&str> {
    response["Response"]["ImageUrls"]
        .as_array()
        .and_then(|urls| urls.first())
        .and_then(Value::as_str)
}

/// Preserve the provider response body so both image generation and settings probes report actionable failures.
pub(super) fn image_provider_error(response: reqwest::blocking::Response) -> AppError {
    let status = response.status();
    let detail = response
        .text()
        .ok()
        .as_deref()
        .and_then(provider_error_detail)
        .or_else(|| translated_http_status(status.as_u16()).map(str::to_owned))
        .unwrap_or_else(|| "服务商未返回可识别的错误详情，请检查模型配置后重试。".to_owned());
    AppError::External(format!("图片模型请求失败：{detail}"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{tencent_image_payload, tencent_image_status, tencent_image_url};

    #[test]
    fn tencent_mps_image_payload_uses_model_parts_and_prompt_limit() {
        let prompt = "画面".repeat(600);
        let payload = tencent_image_payload("Hunyuan:3.0", &prompt, &[]).expect("payload");

        assert_eq!(payload["ModelName"], "Hunyuan");
        assert_eq!(payload["ModelVersion"], "3.0");
        assert!(payload["Prompt"]
            .as_str()
            .is_some_and(|value| value.chars().count() == 1000));
        assert_eq!(payload["EnhancePrompt"], true);
    }

    #[test]
    fn tencent_mps_image_response_reads_done_status_and_image_urls() {
        let response =
            json!({"Response":{"Status":"DONE","ImageUrls":["https://example.com/image.png"]}});

        assert_eq!(tencent_image_status(&response), "done");
        assert_eq!(
            tencent_image_url(&response),
            Some("https://example.com/image.png")
        );
    }

    #[test]
    fn tencent_mps_image_rejects_unsupported_reference_images_before_billing() {
        let error = tencent_image_payload(
            "Hunyuan:3.0",
            "测试图",
            &["https://example.com/reference.png".to_owned()],
        )
        .expect_err("reference images should be rejected");

        assert!(error.to_string().contains("仅支持文生图"));
    }
}
