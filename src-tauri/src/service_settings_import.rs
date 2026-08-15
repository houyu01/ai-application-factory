//! Bulk configuration import: translate the documented Chinese JSON contract, probe it, then save atomically.

use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    providers::ProviderClient,
    value::string,
};

use super::DesktopService;

const INVITE_BASE: &str = "https://monkey-1256112104.cos.ap-chengdu.myqcloud.com/configs";

impl DesktopService {
    /// Import the five Settings cards from local JSON or a six-character invite code.
    /// The frontend triggers this from either header action; no saved setting changes until every real probe succeeds.
    pub fn import_settings(&self, values: Map<String, Value>) -> AppResult<Value> {
        let document = if let Some(config) = values.get("config") {
            config.clone()
        } else {
            let code = string(&values, "invite_code", "");
            if code.len() != 6
                || !code
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
            {
                return Err(AppError::BadRequest(
                    "邀请码必须是 6 位字母或数字".to_owned(),
                ));
            }
            let url = format!("{INVITE_BASE}/{code}/config.json");
            reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .map_err(|error| AppError::External(format!("邀请码下载客户端创建失败：{error}")))?
                .get(url)
                .send()
                .map_err(|error| AppError::External(format!("邀请码配置下载失败：{error}")))?
                .error_for_status()
                .map_err(|error| AppError::External(format!("未找到可用的邀请码配置：{error}")))?
                .json::<Value>()
                .map_err(|error| {
                    AppError::BadRequest(format!("邀请码配置不是有效 JSON：{error}"))
                })?
        };
        self.import_settings_document(document)
    }

    fn import_settings_document(&self, document: Value) -> AppResult<Value> {
        let root = document
            .as_object()
            .ok_or_else(|| AppError::BadRequest("配置 JSON 顶层必须是对象".to_owned()))?;
        let mut candidates = Vec::new();
        for (label, kind) in [
            ("语言模型", "language"),
            ("图像模型", "multimodal"),
            ("视频模型", "video"),
            ("音频模型", "audio"),
        ] {
            let imported = imported_model(root, label, kind)?;
            candidates.push(self.repository.model_config_candidate(&imported)?);
        }
        let storage_values = imported_storage(root)?;
        let storage = self.repository.storage_config_candidate(&storage_values)?;

        let client = ProviderClient::for_model_probe(self.repository.clone(), self.media.clone())?;
        for candidate in &candidates {
            client.probe_model_config(candidate).map_err(|error| {
                AppError::External(format!(
                    "{}校验失败：{error}",
                    model_label(string(candidate, "kind", ""))
                ))
            })?;
        }
        self.media
            .probe(storage.clone())
            .map_err(|error| AppError::External(format!("存储配置校验失败：{error}")))?;

        let response = self
            .repository
            .save_imported_settings(candidates.clone(), storage)?;
        for candidate in candidates {
            let kind = string(&candidate, "kind", "");
            let concurrency = candidate["generation_concurrency"].as_u64().unwrap_or(2) as usize;
            self.worker.set_queue_concurrency(&kind, concurrency);
        }
        Ok(response)
    }
}

fn imported_model(
    root: &Map<String, Value>,
    label: &str,
    kind: &str,
) -> AppResult<Map<String, Value>> {
    let providers = required_object(root, label)?;
    if providers.len() != 1 {
        return Err(AppError::BadRequest(format!(
            "{label}必须且只能包含一个服务商"
        )));
    }
    let (provider_label, value) = providers.iter().next().expect("one provider");
    let provider = match provider_label.as_str() {
        "火山引擎" => "ark",
        "阿里云" | "阿里云 DashScope" => "dashscope",
        "腾讯云" => "tencent",
        _ => {
            return Err(AppError::BadRequest(format!(
                "{label}包含不支持的服务商：{provider_label}"
            )))
        }
    };
    let source = value
        .as_object()
        .ok_or_else(|| AppError::BadRequest(format!("{label}的服务商配置必须是对象")))?;
    let endpoint = required_string(source, "endpoint", label)?;
    let api_key = required_string(source, "apikey", label)?;
    let model = required_string(source, "模型", label)?;
    let concurrency = imported_concurrency(source, label)?;
    let mut result = Map::from_iter([
        ("kind".to_owned(), json!(kind)),
        ("provider".to_owned(), json!(provider)),
        ("api_key".to_owned(), json!(api_key)),
        ("model".to_owned(), json!(model)),
        ("models".to_owned(), json!([model])),
        ("generation_concurrency".to_owned(), json!(concurrency)),
    ]);
    if kind == "video" {
        result.insert("create_url".to_owned(), json!(endpoint));
        result.insert("query_url".to_owned(), json!(format!("{endpoint}/{{id}}")));
    } else {
        result.insert("endpoint".to_owned(), json!(endpoint));
    }
    Ok(result)
}

fn imported_storage(root: &Map<String, Value>) -> AppResult<Map<String, Value>> {
    let media = required_object(root, "媒体存储")?;
    let source = required_object(media, "存储腾讯云cos")?;
    Ok(Map::from_iter([
        ("provider".to_owned(), json!("cos")),
        (
            "secret_id".to_owned(),
            json!(required_string(source, "SecretId", "存储腾讯云cos")?),
        ),
        (
            "secret_key".to_owned(),
            json!(required_string(source, "SecretKey", "存储腾讯云cos")?),
        ),
        (
            "endpoint".to_owned(),
            json!(required_string(source, "endpoint", "存储腾讯云cos")?),
        ),
        (
            "bucket".to_owned(),
            json!(required_string(source, "桶", "存储腾讯云cos")?),
        ),
        (
            "public_base_url".to_owned(),
            json!(required_string(source, "公开可访问域名", "存储腾讯云cos")?),
        ),
        ("prefix".to_owned(), json!("media")),
    ]))
}

fn required_object<'a>(
    root: &'a Map<String, Value>,
    key: &str,
) -> AppResult<&'a Map<String, Value>> {
    root.get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::BadRequest(format!("缺少或无效的“{key}”配置")))
}

fn required_string(source: &Map<String, Value>, key: &str, section: &str) -> AppResult<String> {
    let value = string(source, key, "");
    if value.is_empty() {
        Err(AppError::BadRequest(format!("{section}缺少 {key}")))
    } else {
        Ok(value)
    }
}

fn imported_concurrency(source: &Map<String, Value>, section: &str) -> AppResult<i64> {
    let Some(value) = source.get("concurrency") else {
        return Ok(2);
    };
    value
        .as_i64()
        .or_else(|| value.as_str()?.trim().parse::<i64>().ok())
        .filter(|value| (1..=8).contains(value))
        .ok_or_else(|| {
            AppError::BadRequest(format!("{section}的 concurrency 必须是 1 到 8 的整数"))
        })
}

fn model_label(kind: String) -> &'static str {
    match kind.as_str() {
        "language" => "语言模型",
        "multimodal" => "图像模型",
        "video" => "视频模型",
        "audio" => "音频模型",
        _ => "模型",
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{SocketAddr, TcpListener, TcpStream},
        sync::{Arc, Mutex},
        thread,
        time::Duration,
    };

    use super::{imported_model, imported_storage, DesktopService};
    use crate::{db::Database, media::MediaStore, repository::Repository, worker::DurableWorker};
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn documented_chinese_fields_map_to_internal_model_and_storage_contracts() {
        let document = json!({
            "语言模型":{"火山引擎":{"endpoint":"https://example.com/v3","apikey":"test-key","concurrency":"4","模型":"text-model"}},
            "视频模型":{"火山引擎":{"endpoint":"https://example.com/tasks","apikey":"test-key","concurrency":2,"模型":"video-model"}},
            "媒体存储":{"存储腾讯云cos":{"SecretId":"test-id","SecretKey":"test-secret","endpoint":"https://bucket.example.com","桶":"bucket-1","公开可访问域名":"https://cdn.example.com"}}
        });
        let root = document.as_object().unwrap();
        let language = imported_model(root, "语言模型", "language").unwrap();
        let video = imported_model(root, "视频模型", "video").unwrap();
        let storage = imported_storage(root).unwrap();

        assert_eq!(language["provider"], "ark");
        assert_eq!(language["endpoint"], "https://example.com/v3");
        assert_eq!(language["generation_concurrency"], 4);
        assert_eq!(video["create_url"], "https://example.com/tasks");
        assert_eq!(video["query_url"], "https://example.com/tasks/{id}");
        assert_eq!(video["generation_concurrency"], 2);
        assert_eq!(storage["provider"], "cos");
        assert_eq!(storage["bucket"], "bucket-1");
    }

    #[test]
    fn missing_one_of_the_five_sections_is_rejected_before_any_setting_is_saved() {
        let service = test_service("missing-section", None);
        let error = service
            .import_settings(
                json!({"config":complete_document("http://127.0.0.1:1", false)})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .unwrap_err();

        assert!(error.to_string().contains("媒体存储"));
        assert_eq!(service.repository.setting("language").unwrap(), json!({}));
        assert_eq!(service.repository.setting("storage").unwrap(), json!({}));
    }

    #[test]
    fn legacy_flat_storage_section_is_rejected() {
        let document = json!({
            "存储腾讯云cos":{"SecretId":"test-id","SecretKey":"test-secret","endpoint":"https://bucket.example.com","桶":"bucket-1","公开可访问域名":"https://cdn.example.com"}
        });

        let error = imported_storage(document.as_object().unwrap()).unwrap_err();

        assert!(error.to_string().contains("媒体存储"));
    }

    #[test]
    fn model_concurrency_must_be_an_integer_from_one_through_eight() {
        for invalid in [json!(null), json!(""), json!("2.5"), json!(0), json!(9)] {
            let document = json!({
                "语言模型":{"火山引擎":{"endpoint":"https://example.com/v3","apikey":"test-key","concurrency":invalid,"模型":"text-model"}}
            });
            let error =
                imported_model(document.as_object().unwrap(), "语言模型", "language").unwrap_err();
            assert!(error.to_string().contains("1 到 8"));
        }
    }

    #[test]
    fn mock_provider_and_cos_probes_save_all_five_settings_together() {
        let (base_url, address, server) = mock_import_server(8);
        let service = test_service("complete-import", Some(address));
        let response = service
            .import_settings(
                json!({"config":complete_document(&base_url, true)})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .unwrap();
        server.join().unwrap();

        assert_eq!(response["status"], "saved");
        for (kind, concurrency) in [
            ("language", 4),
            ("multimodal", 4),
            ("video", 2),
            ("audio", 2),
        ] {
            let saved = service.repository.setting(kind).unwrap();
            assert_eq!(saved["provider"], "ark");
            assert_eq!(saved["api_key"], "mock-key");
            assert_eq!(saved["generation_concurrency"], concurrency);
        }
        let storage = service.repository.setting("storage").unwrap();
        assert_eq!(storage["provider"], "cos");
        assert_eq!(storage["bucket"], "mock");
    }

    fn complete_document(base_url: &str, include_storage: bool) -> serde_json::Value {
        let mut document = json!({
            "语言模型":{"火山引擎":{"endpoint":format!("{base_url}/api/plan/v3"),"apikey":"mock-key","concurrency":"4","模型":"text-model"}},
            "图像模型":{"火山引擎":{"endpoint":format!("{base_url}/images/generations"),"apikey":"mock-key","concurrency":4,"模型":"image-model"}},
            "视频模型":{"火山引擎":{"endpoint":format!("{base_url}/video/tasks"),"apikey":"mock-key","concurrency":"2","模型":"video-model"}},
            "音频模型":{"火山引擎":{"endpoint":format!("{base_url}/audio"),"apikey":"mock-key","concurrency":2,"模型":"audio-model"}}
        });
        if include_storage {
            let storage_endpoint = base_url.replacen("127.0.0.1", "local", 1);
            document["媒体存储"] = json!({
                "存储腾讯云cos":{
                    "SecretId":"mock-id","SecretKey":"mock-secret","endpoint":storage_endpoint,
                    "桶":"mock","公开可访问域名":base_url
                }
            });
        }
        document
    }

    fn test_service(label: &str, storage_address: Option<SocketAddr>) -> DesktopService {
        let root = std::env::temp_dir().join(format!(
            "ai-factory-settings-import-{label}-{}",
            Uuid::new_v4()
        ));
        let database = Database::open(root.join("test.db")).unwrap();
        let repository = Repository::new(database);
        let media = if let Some(address) = storage_address {
            let media_root = root.join("media");
            std::fs::create_dir_all(&media_root).unwrap();
            MediaStore {
                repository: repository.clone(),
                root: media_root.canonicalize().unwrap(),
                client: reqwest::blocking::Client::builder()
                    .resolve("mock.local", address)
                    .timeout(Duration::from_secs(5))
                    .build()
                    .unwrap(),
            }
        } else {
            MediaStore::new(repository.clone()).unwrap()
        };
        let worker = DurableWorker::new(repository.clone(), media.clone()).unwrap();
        DesktopService {
            repository,
            media,
            worker,
        }
    }

    fn mock_import_server(
        expected_requests: usize,
    ) -> (String, SocketAddr, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let uploaded = Arc::new(Mutex::new(Vec::new()));
        let handle = thread::spawn(move || {
            for stream in listener.incoming().take(expected_requests) {
                respond(stream.unwrap(), &uploaded);
            }
        });
        (format!("http://{address}"), address, handle)
    }

    fn respond(mut stream: TcpStream, uploaded: &Arc<Mutex<Vec<u8>>>) {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        let header_end = loop {
            let count = stream.read(&mut buffer).unwrap();
            request.extend_from_slice(&buffer[..count]);
            if let Some(index) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                break index + 4;
            }
        };
        let headers = String::from_utf8_lossy(&request[..header_end]).into_owned();
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(str::trim)
                    .and_then(|value| value.parse::<usize>().ok())
            })
            .unwrap_or(0);
        while request.len() < header_end + content_length {
            let count = stream.read(&mut buffer).unwrap();
            request.extend_from_slice(&buffer[..count]);
        }
        let request_line = headers.lines().next().unwrap_or_default();
        let (status, content_type, body) = if request_line
            .starts_with("POST /api/plan/v3/chat/completions ")
        {
            (
                "200 OK",
                "application/json",
                br#"{"choices":[{"message":{"content":"OK"}}]}"#.to_vec(),
            )
        } else if request_line.starts_with("POST /images/generations ")
            || request_line.starts_with("POST /video/tasks ")
        {
            (
                "400 Bad Request",
                "application/json",
                br#"{"error":{"message":"mock validation"}}"#.to_vec(),
            )
        } else if request_line.starts_with("POST /audio ") {
            (
                "200 OK",
                "application/json",
                br#"{"code":0,"data":"AQID"}"#.to_vec(),
            )
        } else if request_line.starts_with("PUT /media/probe-") {
            *uploaded.lock().unwrap() = request[header_end..header_end + content_length].to_vec();
            ("200 OK", "text/plain", Vec::new())
        } else if request_line.starts_with("GET /media/probe-") {
            ("200 OK", "text/plain", uploaded.lock().unwrap().clone())
        } else if request_line.starts_with("DELETE /media/probe-") {
            ("204 No Content", "text/plain", Vec::new())
        } else {
            (
                "404 Not Found",
                "text/plain",
                request_line.as_bytes().to_vec(),
            )
        };
        write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
        stream.write_all(&body).unwrap();
    }
}
