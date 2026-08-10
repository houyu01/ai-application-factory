//! Local-first media storage with compatible optional TOS/COS/OSS S3 object operations.

use std::{
    fs,
    path::{Path, PathBuf},
};

use base64::Engine;
use chrono::Utc;
use reqwest::{blocking::Client, Method};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use url::Url;

use crate::{
    error::{AppError, AppResult},
    repository::Repository,
    storage::{encode_key, signing_key, StorageConfig},
    value::new_id,
};

/// Owns generated media files and intentionally limits cloud calls to opt-in object storage settings.
#[derive(Clone)]
pub struct MediaStore {
    repository: Repository,
    root: PathBuf,
    client: Client,
}

impl MediaStore {
    /// Create the media boundary adjacent to the app-owned database rather than the application bundle.
    pub fn new(repository: Repository) -> AppResult<Self> {
        let root = repository
            .db
            .path()
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("media");
        fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(180))
            .build()
            .map_err(|error| AppError::External(error.to_string()))?;
        Ok(Self {
            repository,
            root,
            client,
        })
    }

    /// Decode a browser data URL and persist it through the selected local or S3-compatible store.
    pub fn save_data_url(&self, value: &str) -> AppResult<String> {
        let (header, encoded) = value
            .split_once(',')
            .ok_or_else(|| AppError::BadRequest("上传内容不是有效 data URL".to_owned()))?;
        if !header.starts_with("data:") || !header.ends_with(";base64") {
            return Err(AppError::BadRequest(
                "上传内容必须是 base64 data URL".to_owned(),
            ));
        }
        let content_type = header
            .trim_start_matches("data:")
            .trim_end_matches(";base64");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| AppError::BadRequest("上传图片不是有效 base64 数据".to_owned()))?;
        self.save(
            &bytes,
            extension_for_content_type(content_type),
            content_type,
        )
    }

    /// Download a model-provider URL before persisting it to the configured user-controlled storage target.
    pub fn save_url(&self, url: &str, extension: &str) -> AppResult<String> {
        if url.starts_with("/api/media/") {
            return Ok(url.to_owned());
        }
        let parsed = Url::parse(url)
            .map_err(|_| AppError::BadRequest("媒体结果 URL 必须是 http 或 https".to_owned()))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(AppError::BadRequest(
                "媒体结果 URL 必须是 http 或 https".to_owned(),
            ));
        }
        let response = self
            .client
            .get(url)
            .header("User-Agent", "ai-application-factory/desktop")
            .send()
            .map_err(|error| AppError::External(format!("下载模型媒体失败：{error}")))?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/octet-stream")
            .split(';')
            .next()
            .unwrap_or("application/octet-stream")
            .to_owned();
        let bytes = response
            .bytes()
            .map_err(|error| AppError::External(format!("读取模型媒体失败：{error}")))?;
        self.save(&bytes, extension, &content_type)
    }

    /// Save raw content and return either the local API path or a remote object URL used by the existing UI.
    pub fn save(&self, content: &[u8], extension: &str, content_type: &str) -> AppResult<String> {
        let extension = if extension.starts_with('.') {
            extension.to_owned()
        } else {
            format!(".{extension}")
        };
        let media_id = format!("{}{}", new_id().replace('-', ""), extension);
        let config = self.config()?;
        if config.provider == "local" {
            fs::write(self.root.join(&media_id), content)?;
            return Ok(format!("/api/media/{media_id}"));
        }
        let key = format!("{}/{}", config.prefix, media_id);
        self.s3_request(&config, Method::PUT, &key, Some(content), content_type)?;
        Ok(config.object_url(&key))
    }

    /// Resolve a local custom-protocol id safely; remote URLs are supplied directly to the web view.
    pub fn path_for(&self, media_id: &str) -> Option<PathBuf> {
        let candidate = self.root.join(media_id).canonicalize().ok()?;
        if !candidate.starts_with(&self.root) || !candidate.is_file() {
            return None;
        }
        Some(candidate)
    }

    /// Remove local or owned remote media during project/history deletion without touching arbitrary URLs.
    pub fn delete_url(&self, value: Option<&str>) -> AppResult<bool> {
        let Some(value) = value.filter(|item| !item.is_empty()) else {
            return Ok(false);
        };
        let config = self.config()?;
        if config.provider == "local" {
            let Some(id) = value
                .strip_prefix("/api/media/")
                .map(|item| item.split('?').next().unwrap_or(item))
            else {
                return Ok(false);
            };
            let Some(path) = self.path_for(id) else {
                return Ok(false);
            };
            fs::remove_file(path)?;
            return Ok(true);
        }
        let Some(key) = config.key_for_url(value) else {
            return Ok(false);
        };
        self.s3_request(&config, Method::DELETE, &key, None, "")?;
        Ok(true)
    }

    /// Verify object-store credentials by upload, public download, and cleanup while leaving the active setting untouched.
    pub fn probe(&self, values: Map<String, Value>) -> AppResult<()> {
        let config = StorageConfig::from_values(values)?;
        if config.provider == "local" {
            return Ok(());
        }
        let key = format!("{}/probe-{}.txt", config.prefix, new_id());
        let content = b"ai-application-factory-storage-probe";
        self.s3_request(&config, Method::PUT, &key, Some(content), "text/plain")?;
        let url = config.object_url(&key);
        let outcome = self.client.get(&url).header("Cache-Control", "no-cache").send().and_then(|response| response.error_for_status()).and_then(|response| response.bytes()).map_err(|error| AppError::BadRequest(format!("{} 媒体存储嗅探访问失败：{error}。请检查 Bucket 公开读权限或公开访问域名/CDN 配置", config.provider.to_uppercase()))).and_then(|bytes| if bytes.as_ref() == content { Ok(()) } else { Err(AppError::BadRequest("媒体存储嗅探访问失败：下载内容与上传内容不一致".to_owned())) });
        let _ = self.s3_request(&config, Method::DELETE, &key, None, "");
        outcome
    }

    /// Return a provider-reachable reference; local desktop files require an explicitly configured public URL.
    pub fn provider_reference_url(&self, value: &str) -> Option<String> {
        if value.starts_with("http://") || value.starts_with("https://") {
            return public_url(value).then_some(value.to_owned());
        }
        let config = self.config().ok()?;
        if config.provider == "local" && value.starts_with("/api/media/") {
            let id = value
                .strip_prefix("/api/media/")?
                .split('?')
                .next()
                .unwrap_or_default();
            let path = self.path_for(id)?;
            let bytes = fs::read(&path).ok()?;
            let content_type = mime_guess::from_path(path)
                .first_or_octet_stream()
                .essence_str()
                .to_owned();
            return Some(format!(
                "data:{content_type};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ));
        }
        let base = config.public_base_url;
        if config.provider == "local" && value.starts_with("/api/media/") && public_url(&base) {
            Some(format!("{}{}", base.trim_end_matches('/'), value))
        } else {
            None
        }
    }

    fn config(&self) -> AppResult<StorageConfig> {
        StorageConfig::from_values(
            self.repository
                .setting("storage")?
                .as_object()
                .cloned()
                .unwrap_or_default(),
        )
    }

    fn s3_request(
        &self,
        config: &StorageConfig,
        method: Method,
        key: &str,
        body: Option<&[u8]>,
        content_type: &str,
    ) -> AppResult<()> {
        let endpoint = config.service_endpoint()?;
        let host = endpoint
            .host_str()
            .ok_or_else(|| AppError::BadRequest("存储 endpoint 缺少主机".to_owned()))?;
        let host = if let Some(port) = endpoint.port() {
            format!("{host}:{port}")
        } else {
            host.to_owned()
        };
        let object_url = format!(
            "{}://{}/{}",
            endpoint.scheme(),
            config.bucket_host(&host),
            encode_key(key)
        );
        let payload = body.unwrap_or_default();
        let hash = hex::encode(Sha256::digest(payload));
        let date = Utc::now();
        let amz_date = date.format("%Y%m%dT%H%M%SZ").to_string();
        let day = date.format("%Y%m%d").to_string();
        let signed_host = config.bucket_host(&host);
        let headers=format!("content-type:{content_type}\nhost:{signed_host}\nx-amz-content-sha256:{hash}\nx-amz-date:{amz_date}\n");
        let signed = "content-type;host;x-amz-content-sha256;x-amz-date";
        let canonical = format!(
            "{}\n/{}\n\n{}\n{}\n{}",
            method.as_str(),
            encode_key(key),
            headers,
            signed,
            hash
        );
        let scope = format!("{day}/{}/s3/aws4_request", config.region());
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
            hex::encode(Sha256::digest(canonical.as_bytes()))
        );
        let signature = hex::encode(signing_key(
            &config.secret_key,
            &day,
            config.region(),
            "s3",
            &string_to_sign,
        ));
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed}, Signature={signature}",
            config.secret_id
        );
        let request = self
            .client
            .request(method, &object_url)
            .header("host", signed_host)
            .header("content-type", content_type)
            .header("x-amz-content-sha256", hash)
            .header("x-amz-date", amz_date)
            .header("authorization", authorization)
            .body(payload.to_vec());
        request
            .send()
            .map_err(|error| {
                AppError::External(format!(
                    "{} 对象存储请求失败：{error}",
                    config.provider.to_uppercase()
                ))
            })?
            .error_for_status()
            .map_err(|error| {
                AppError::External(format!(
                    "{} 对象存储请求失败：{error}",
                    config.provider.to_uppercase()
                ))
            })?;
        Ok(())
    }
}

fn extension_for_content_type(value: &str) -> &str {
    if value.contains("png") {
        ".png"
    } else if value.contains("jpeg") || value.contains("jpg") {
        ".jpg"
    } else if value.contains("webp") {
        ".webp"
    } else {
        ".bin"
    }
}
fn public_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some_and(|host| {
            host != "localhost" && !host.ends_with(".local") && !host.ends_with(".internal")
        })
}
