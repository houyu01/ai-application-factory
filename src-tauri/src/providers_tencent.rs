//! Tencent MPS TC3 signing and media-generation operations.

use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::header::CONTENT_TYPE;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use url::Url;

use crate::{
    error::{AppError, AppResult},
    providers::{ProviderClient, VideoJob},
};

type HmacSha256 = Hmac<Sha256>;

impl ProviderClient {
    /// Submit a Tencent MPS video task from the durable video worker and retain its remote task ID.
    pub(super) fn start_tencent(
        &self,
        config: &Map<String, Value>,
        model: &str,
        prompt: &str,
        ratio: &str,
        resolution: &str,
        seconds: i64,
        references: &[String],
        reference_video: Option<&str>,
    ) -> AppResult<VideoJob> {
        let (name, version) = tencent_model_parts(model, model, "");
        let mut payload = json!({"ModelName":name,"Prompt":prompt.chars().take(2000).collect::<String>(),"Duration":seconds,"ExtraParameters":{"Resolution":resolution.to_uppercase(),"AspectRatio":ratio}});
        if !version.is_empty() {
            payload["ModelVersion"] = json!(version);
        }
        let images = unique(references);
        if !images.is_empty() {
            payload["ImageInfos"] = json!(images
                .into_iter()
                .map(|image| json!({"ImageUrl":image}))
                .collect::<Vec<_>>());
        }
        if let Some(video) = reference_video.filter(|value| !value.is_empty()) {
            payload["VideoInfos"] =
                json!([{"VideoUrl":video,"ReferType":"base","KeepOriginalSound":"no"}]);
        }
        let response = self.tencent_request(config, "CreateAigcVideoTask", &payload)?;
        self.read_submission(&response, "腾讯云")
    }

    pub(super) fn poll_tencent(
        &self,
        config: &Map<String, Value>,
        task_id: &str,
    ) -> AppResult<VideoJob> {
        let response =
            self.tencent_request(config, "DescribeAigcVideoTask", &json!({"TaskId":task_id}))?;
        self.read_poll(&response, "腾讯云", task_id)
    }

    /// Verify Tencent MPS image credentials without creating a billable image-generation task.
    pub(super) fn probe_tencent_image_credentials(
        &self,
        config: &Map<String, Value>,
    ) -> AppResult<()> {
        match self.tencent_request(
            config,
            "DescribeAigcImageTask",
            &json!({"TaskId":"probe-invalid-task"}),
        ) {
            Ok(_) => Ok(()),
            Err(AppError::External(message))
                if ["InvalidParameter", "ResourceNotFound", "FailedOperation"]
                    .into_iter()
                    .any(|code| message.contains(code)) =>
            {
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// Sign and send one MPS API 3.0 request for the media adapters and configuration probes.
    pub(super) fn tencent_request(
        &self,
        config: &Map<String, Value>,
        action: &str,
        payload: &Value,
    ) -> AppResult<Value> {
        let secret_id = config
            .get("secret_id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::BadRequest("腾讯云 MPS 需要配置 SecretId".to_owned()))?;
        let secret_key = config
            .get("secret_key")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AppError::BadRequest("腾讯云 MPS 需要配置 SecretKey".to_owned()))?;
        let endpoint = config
            .get("endpoint")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or("https://mps.tencentcloudapi.com");
        let host = Url::parse(endpoint)
            .map_err(|_| AppError::BadRequest("腾讯云 endpoint 无效".to_owned()))?
            .host_str()
            .ok_or_else(|| AppError::BadRequest("腾讯云 endpoint 缺少主机".to_owned()))?
            .to_owned();
        let body = serde_json::to_string(payload).map_err(AppError::from)?;
        let now = Utc::now();
        let stamp = now.timestamp();
        let date = now.format("%Y-%m-%d").to_string();
        let headers = format!(
            "content-type:application/json\nhost:{host}\nx-tc-action:{}\n",
            action.to_lowercase()
        );
        let signed = "content-type;host;x-tc-action";
        let canonical = format!(
            "POST\n/\n\n{headers}\n{signed}\n{}",
            hex::encode(Sha256::digest(body.as_bytes()))
        );
        let scope = format!("{date}/mps/tc3_request");
        let string = format!(
            "TC3-HMAC-SHA256\n{stamp}\n{scope}\n{}",
            hex::encode(Sha256::digest(canonical.as_bytes()))
        );
        let secret_date = hmac(format!("TC3{secret_key}").as_bytes(), &date);
        let secret_service = hmac(&secret_date, "mps");
        let signing = hmac(&secret_service, "tc3_request");
        let signature = hex::encode(hmac(&signing, &string));
        let authorization = format!("TC3-HMAC-SHA256 Credential={secret_id}/{scope}, SignedHeaders={signed}, Signature={signature}");
        let result = self
            .client
            .post(endpoint)
            .header("Authorization", authorization)
            .header(CONTENT_TYPE, "application/json")
            .header("Host", host)
            .header("X-TC-Action", action)
            .header(
                "X-TC-Region",
                config
                    .get("region")
                    .and_then(Value::as_str)
                    .unwrap_or("ap-guangzhou"),
            )
            .header("X-TC-Timestamp", stamp.to_string())
            .header("X-TC-Version", "2019-06-12")
            .body(body)
            .send()
            .map_err(|error| AppError::External(format!("腾讯云 MPS 请求失败：{error}")))?
            .error_for_status()
            .map_err(|error| AppError::External(format!("腾讯云 MPS 请求失败：{error}")))?
            .json::<Value>()
            .map_err(|error| AppError::External(format!("腾讯云 MPS 响应无效：{error}")))?;
        if let Some(error) = result["Response"]["Error"].as_object() {
            return Err(AppError::External(format!(
                "腾讯云 MPS API 请求失败（{}）：{}",
                error
                    .get("Code")
                    .and_then(Value::as_str)
                    .unwrap_or("Unknown"),
                error
                    .get("Message")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )));
        }
        Ok(result)
    }
}

/// Split the workbench's `厂商模型:版本` selection into the MPS request fields.
pub(super) fn tencent_model_parts(
    model: &str,
    fallback_name: &str,
    fallback_version: &str,
) -> (String, String) {
    let (name, version) = model
        .trim()
        .split_once(':')
        .or_else(|| model.trim().split_once('/'))
        .unwrap_or((model.trim(), ""));
    (
        if name.is_empty() {
            fallback_name.to_owned()
        } else {
            name.to_owned()
        },
        if version.is_empty() {
            fallback_version.to_owned()
        } else {
            version.to_owned()
        },
    )
}

fn unique(values: &[String]) -> Vec<String> {
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
fn hmac(key: &[u8], value: &str) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts arbitrary key lengths");
    mac.update(value.as_bytes());
    mac.finalize().into_bytes().to_vec()
}
