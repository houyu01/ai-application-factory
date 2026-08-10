//! Optional S3-compatible storage configuration and request-signing primitives.

use hmac::{Hmac, Mac};
use serde_json::{Map, Value};
use sha2::Sha256;
use url::Url;

use crate::{
    error::{AppError, AppResult},
    value::string,
};

type HmacSha256 = Hmac<Sha256>;

/// Validates the persisted local/TOS/COS/OSS configuration and identifies URLs owned by it.
#[derive(Clone)]
pub(crate) struct StorageConfig {
    pub(crate) provider: String,
    pub(crate) endpoint: String,
    pub(crate) bucket: String,
    pub(crate) region_name: String,
    pub(crate) secret_id: String,
    pub(crate) secret_key: String,
    pub(crate) prefix: String,
    pub(crate) public_base_url: String,
}

impl StorageConfig {
    pub(crate) fn from_values(values: Map<String, Value>) -> AppResult<Self> {
        let provider = string(&values, "provider", "local").to_lowercase();
        if !["local", "tos", "cos", "oss"].contains(&provider.as_str()) {
            return Err(AppError::BadRequest(
                "storage provider must be one of: local, tos, cos, oss".to_owned(),
            ));
        }
        let bucket = string(&values, "bucket", "");
        let endpoint = normalise_endpoint(&string(&values, "endpoint", ""), &bucket)?;
        let result = Self {
            provider: provider.clone(),
            endpoint,
            bucket,
            region_name: string(&values, "region", ""),
            secret_id: string(&values, "secret_id", ""),
            secret_key: string(&values, "secret_key", ""),
            prefix: string(&values, "prefix", "media")
                .trim_matches('/')
                .to_owned(),
            public_base_url: string(&values, "public_base_url", "")
                .trim_end_matches('/')
                .to_owned(),
        };
        if provider != "local" {
            let missing = [
                ("endpoint", result.endpoint.is_empty()),
                ("bucket", result.bucket.is_empty()),
                ("secret_id", result.secret_id.is_empty()),
                ("secret_key", result.secret_key.is_empty()),
            ]
            .into_iter()
            .filter_map(|(name, empty)| empty.then_some(name))
            .collect::<Vec<_>>();
            if !missing.is_empty() {
                return Err(AppError::BadRequest(format!(
                    "{provider} storage requires: {}",
                    missing.join(", ")
                )));
            }
        }
        Ok(result)
    }

    pub(crate) fn region(&self) -> &str {
        if self.region_name.is_empty() {
            "us-east-1"
        } else {
            &self.region_name
        }
    }

    pub(crate) fn service_endpoint(&self) -> AppResult<Url> {
        Url::parse(&self.endpoint)
            .map_err(|_| AppError::BadRequest("storage endpoint 必须是有效 HTTP(S) URL".to_owned()))
    }

    pub(crate) fn bucket_host(&self, host: &str) -> String {
        if host
            .to_lowercase()
            .starts_with(&format!("{}.", self.bucket.to_lowercase()))
        {
            host.to_owned()
        } else {
            format!("{}.{}", self.bucket, host)
        }
    }

    pub(crate) fn object_url(&self, key: &str) -> String {
        if !self.public_base_url.is_empty() {
            format!("{}/{}", self.public_base_url, encode_key(key))
        } else {
            let endpoint = Url::parse(&self.endpoint).expect("validated endpoint");
            let host = endpoint.host_str().unwrap_or_default();
            let host = if let Some(port) = endpoint.port() {
                format!("{host}:{port}")
            } else {
                host.to_owned()
            };
            format!(
                "{}://{}/{}",
                endpoint.scheme(),
                self.bucket_host(&host),
                encode_key(key)
            )
        }
    }

    pub(crate) fn key_for_url(&self, value: &str) -> Option<String> {
        let url = Url::parse(value).ok()?;
        let key = if !self.public_base_url.is_empty()
            && value.starts_with(&format!("{}/", self.public_base_url))
        {
            value[self.public_base_url.len() + 1..]
                .split('?')
                .next()?
                .to_owned()
        } else {
            let endpoint = Url::parse(&self.endpoint).ok()?;
            let host = endpoint.host_str()?;
            let expected = self.bucket_host(host);
            if url.host_str()? != expected {
                return None;
            }
            url.path().trim_start_matches('/').to_owned()
        };
        let key = urlencoding_decode(&key);
        (key == self.prefix || key.starts_with(&format!("{}/", self.prefix))).then_some(key)
    }
}

fn normalise_endpoint(value: &str, bucket: &str) -> AppResult<String> {
    let mut endpoint = value.trim().trim_end_matches('/').to_owned();
    if endpoint.is_empty() {
        return Ok(endpoint);
    }
    if !endpoint.contains("://") {
        endpoint = format!("https://{endpoint}");
    }
    let url = Url::parse(&endpoint)
        .map_err(|_| AppError::BadRequest("storage endpoint 必须是有效 HTTP(S) URL".to_owned()))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(AppError::BadRequest(
            "storage endpoint 必须是有效 HTTP(S) URL".to_owned(),
        ));
    }
    let host = url.host_str().unwrap_or_default();
    let host = host.strip_prefix(&format!("{bucket}.")).unwrap_or(host);
    let port = url
        .port()
        .map(|value| format!(":{value}"))
        .unwrap_or_default();
    let path = url.path().trim_end_matches('/');
    Ok(format!("{}://{host}{port}{path}", url.scheme())
        .trim_end_matches('/')
        .to_owned())
}

pub(crate) fn signing_key(
    secret: &str,
    day: &str,
    region: &str,
    service: &str,
    value: &str,
) -> Vec<u8> {
    let date = hmac_bytes(format!("AWS4{secret}").as_bytes(), day);
    let region = hmac_bytes(&date, region);
    let service = hmac_bytes(&region, service);
    let signing = hmac_bytes(&service, "aws4_request");
    hmac_bytes(&signing, value)
}

fn hmac_bytes(key: &[u8], value: &str) -> Vec<u8> {
    let mut hmac = HmacSha256::new_from_slice(key).expect("HMAC accepts every key length");
    hmac.update(value.as_bytes());
    hmac.finalize().into_bytes().to_vec()
}

pub(crate) fn encode_key(key: &str) -> String {
    key.split('/')
        .map(|part| {
            url::form_urlencoded::byte_serialize(part.as_bytes())
                .collect::<String>()
                .replace('+', "%20")
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn urlencoding_decode(value: &str) -> String {
    let mut decoded = String::new();
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(hex) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                decoded.push(hex as char);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index] as char);
        index += 1;
    }
    decoded
}
