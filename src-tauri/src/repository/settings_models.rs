//! Per-provider model configuration persistence for the settings workbench.

use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    value::string,
    volcengine_tts::{apply_seed_tts_two_defaults, HTTP_ENDPOINT, RESOURCE_ID},
};

use super::Repository;

const MODEL_KINDS: &[&str] = &["language", "multimodal", "video", "audio"];
const PROVIDERS: &[&str] = &["ark", "dashscope", "tencent"];
/// Persisted JSON map keyed by provider; it retains dormant vendor settings while the root stays active.
const PROVIDER_PROFILES: &str = "provider_profiles";

impl Repository {
    /// Return public settings cards with one non-secret profile for every saved provider.
    pub fn model_configs(&self) -> AppResult<Value> {
        let mut result = Map::new();
        for kind in MODEL_KINDS {
            result.insert((*kind).to_owned(), self.public_model_config(kind)?);
        }
        Ok(Value::Object(result))
    }

    /// Build a single provider profile while retaining other vendors' credentials and settings.
    pub(crate) fn model_config_candidate(
        &self,
        values: &Map<String, Value>,
    ) -> AppResult<Map<String, Value>> {
        let kind = string(values, "kind", "");
        validate_kind(&kind)?;
        let previous = self
            .setting(&kind)?
            .as_object()
            .cloned()
            .unwrap_or_default();
        let provider = string(values, "provider", &string(&previous, "provider", "ark"));
        validate_provider(&provider)?;
        let mut profiles = provider_profiles(&previous);
        let mut stored = profile(&profiles, &provider).unwrap_or_default();

        for (key, value) in values {
            if key == PROVIDER_PROFILES
                || (is_secret(key) && value.as_str().unwrap_or_default().is_empty())
            {
                continue;
            }
            stored.insert(key.clone(), value.clone());
        }
        if kind == "audio" && provider == "ark" {
            apply_seed_tts_two_defaults(&mut stored);
        }
        for (key, value) in provider_defaults(&kind, &provider) {
            if !stored.contains_key(key) || string(&stored, key, "").is_empty() {
                stored.insert(key.to_owned(), json!(value));
            }
        }
        migrate_ark_plan_language_model(&mut stored, &kind, &provider);
        normalize_model_selection(&mut stored)?;
        stored.insert("kind".to_owned(), json!(kind));
        stored.insert("provider".to_owned(), json!(provider));
        profiles.insert(provider, Value::Object(profile_without_index(&stored)));
        stored.insert(PROVIDER_PROFILES.to_owned(), Value::Object(profiles));
        Ok(stored)
    }

    /// Persist a successfully probed active profile together with profiles already saved for other vendors.
    pub fn save_model_config(&self, values: Map<String, Value>) -> AppResult<Value> {
        let kind = string(&values, "kind", "");
        let stored = self.model_config_candidate(&values)?;
        self.set_setting(&kind, Value::Object(stored))?;
        let mut response = self
            .public_model_config(&kind)?
            .as_object()
            .cloned()
            .unwrap_or_default();
        response.insert("status".to_owned(), json!("saved"));
        Ok(Value::Object(response))
    }

    /// Persist model-list edits in the selected vendor profile without a network probe.
    pub fn save_model_options(
        &self,
        kind: &str,
        mut values: Map<String, Value>,
    ) -> AppResult<Value> {
        validate_kind(kind)?;
        let existing = self.setting(kind)?.as_object().cloned().unwrap_or_default();
        values.insert("kind".to_owned(), json!(kind));
        values
            .entry("provider".to_owned())
            .or_insert_with(|| json!(string(&existing, "provider", "ark")));
        let stored = self.model_config_candidate(&values)?;
        self.set_setting(kind, Value::Object(stored))?;
        self.public_model_config(kind)
    }

    /// Reveal a saved provider API key only for the explicit eye-button request in settings.
    pub fn model_api_key(&self, kind: &str, requested_provider: Option<&str>) -> AppResult<Value> {
        validate_kind(kind)
            .map_err(|_| AppError::NotFound(format!("Unsupported model kind: {kind}")))?;
        let config = self.setting(kind)?;
        let object = config.as_object().cloned().unwrap_or_default();
        let provider = requested_provider
            .map(str::to_owned)
            .unwrap_or_else(|| string(&object, "provider", "ark"));
        validate_provider(&provider)?;
        let profiles = provider_profiles(&object);
        let selected = profile(&profiles, &provider).unwrap_or_default();
        let key = string(&selected, "api_key", "");
        if key.is_empty() {
            return Err(AppError::NotFound(format!("{kind} 模型尚未配置 API Key")));
        }
        Ok(json!({"kind":kind,"provider":provider,"api_key":key}))
    }

    fn public_model_config(&self, kind: &str) -> AppResult<Value> {
        validate_kind(kind)?;
        let object = self.setting(kind)?.as_object().cloned().unwrap_or_default();
        let mut response = public_profile(kind, &object);
        let public_profiles: Map<String, Value> = provider_profiles(&object)
            .into_iter()
            .filter_map(|(provider, value)| {
                value
                    .as_object()
                    .map(|profile| (provider, Value::Object(public_profile(kind, profile))))
            })
            .collect();
        response.insert(PROVIDER_PROFILES.to_owned(), Value::Object(public_profiles));
        Ok(Value::Object(response))
    }
}

fn validate_kind(kind: &str) -> AppResult<()> {
    if MODEL_KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(AppError::BadRequest("不支持的模型类型".to_owned()))
    }
}

fn validate_provider(provider: &str) -> AppResult<()> {
    if PROVIDERS.contains(&provider) {
        Ok(())
    } else {
        Err(AppError::BadRequest("不支持的服务商".to_owned()))
    }
}

fn is_secret(key: &str) -> bool {
    ["api_key", "secret_id", "secret_key"].contains(&key)
}

fn provider_profiles(config: &Map<String, Value>) -> Map<String, Value> {
    let mut profiles = config
        .get(PROVIDER_PROFILES)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let provider = string(config, "provider", "ark");
    if PROVIDERS.contains(&provider.as_str()) {
        profiles.insert(provider, Value::Object(profile_without_index(config)));
    }
    profiles
}

fn profile(profiles: &Map<String, Value>, provider: &str) -> Option<Map<String, Value>> {
    profiles
        .get(provider)
        .and_then(Value::as_object)
        .map(profile_without_index)
}

fn profile_without_index(config: &Map<String, Value>) -> Map<String, Value> {
    config
        .iter()
        .filter(|(key, _)| key.as_str() != PROVIDER_PROFILES)
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn normalize_model_selection(stored: &mut Map<String, Value>) -> AppResult<()> {
    let mut models = stored
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().map(str::trim))
        .filter(|item| !item.is_empty())
        .fold(Vec::new(), |mut all, item| {
            if !all.iter().any(|saved| saved == item) {
                all.push(item.to_owned());
            }
            all
        });
    let model = string(
        stored,
        "model",
        models.first().map(String::as_str).unwrap_or(""),
    );
    if !model.is_empty() && !models.contains(&model) {
        models.insert(0, model.clone());
    }
    stored.insert("model".to_owned(), json!(model));
    stored.insert("models".to_owned(), json!(models));
    let concurrency = stored
        .get("generation_concurrency")
        .and_then(Value::as_i64)
        .unwrap_or(2);
    if !(1..=8).contains(&concurrency) {
        return Err(AppError::BadRequest(
            "生成并发数必须在 1 到 8 之间".to_owned(),
        ));
    }
    stored.insert("generation_concurrency".to_owned(), json!(concurrency));
    Ok(())
}

fn public_profile(kind: &str, object: &Map<String, Value>) -> Map<String, Value> {
    let defaults = model_defaults(kind);
    let models = object
        .get("models")
        .and_then(Value::as_array)
        .cloned()
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| defaults.1.into_iter().map(Value::String).collect());
    let model = string(
        object,
        "model",
        models.first().and_then(Value::as_str).unwrap_or_default(),
    );
    let key = string(object, "api_key", "");
    Map::from_iter([
        ("kind".to_owned(), json!(kind)),
        ("endpoint".to_owned(), field(object, "endpoint")),
        ("model".to_owned(), json!(model)),
        ("models".to_owned(), Value::Array(models)),
        ("api_key_set".to_owned(), json!(!key.is_empty())),
        ("api_key_masked".to_owned(), json!(mask(&key))),
        ("create_url".to_owned(), field(object, "create_url")),
        ("query_url".to_owned(), field(object, "query_url")),
        (
            "provider".to_owned(),
            json!(string(object, "provider", "ark")),
        ),
        ("region".to_owned(), field(object, "region")),
        (
            "secret_id_masked".to_owned(),
            json!(mask(&string(object, "secret_id", ""))),
        ),
        (
            "secret_key_set".to_owned(),
            json!(!string(object, "secret_key", "").is_empty()),
        ),
        ("app_id".to_owned(), field(object, "app_id")),
        ("resource_id".to_owned(), field(object, "resource_id")),
        ("voice".to_owned(), field(object, "voice")),
        (
            "generation_concurrency".to_owned(),
            object
                .get("generation_concurrency")
                .cloned()
                .unwrap_or_else(|| json!(2)),
        ),
    ])
}

fn field(object: &Map<String, Value>, key: &str) -> Value {
    object.get(key).cloned().unwrap_or_else(|| json!(""))
}

fn mask(value: &str) -> String {
    (!value.is_empty())
        .then(|| "*".repeat(value.len().clamp(8, 16)))
        .unwrap_or_default()
}

fn model_defaults(kind: &str) -> (&'static str, Vec<String>) {
    match kind {
        "language" => (
            "doubao-seed-2.1-turbo",
            vec![
                "doubao-seed-2.1-turbo".into(),
                "doubao-seed-1-6-250615".into(),
                "qwen-plus".into(),
                "hunyuan-turbos-latest".into(),
            ],
        ),
        "multimodal" => (
            "doubao-seedream-4-0-250828",
            vec![
                "doubao-seedream-4-0-250828".into(),
                "qwen-image-2.0".into(),
                "qwen-image-2.0-pro".into(),
                "Hunyuan:3.0".into(),
            ],
        ),
        "video" => (
            "doubao-seedance-2.0",
            vec![
                "doubao-seedance-2.0".into(),
                "wan2.6-r2v-flash".into(),
                "Hunyuan:1.5".into(),
            ],
        ),
        _ => (RESOURCE_ID, vec![RESOURCE_ID.into()]),
    }
}

fn provider_defaults(kind: &str, provider: &str) -> Vec<(&'static str, &'static str)> {
    match (kind, provider) {
        ("language", "ark") => vec![("endpoint", "https://ark.cn-beijing.volces.com/api/v3"), ("model", "doubao-seed-2.1-turbo")],
        ("language", "dashscope") => vec![("endpoint", "https://dashscope.aliyuncs.com/compatible-mode/v1"), ("model", "qwen-plus")],
        ("language", "tencent") => vec![("endpoint", "https://api.hunyuan.cloud.tencent.com/v1"), ("model", "hunyuan-turbos-latest")],
        ("multimodal", "ark") => vec![("endpoint", "https://ark.cn-beijing.volces.com/api/plan/v3"), ("model", "doubao-seedream-4-0-250828")],
        ("multimodal", "dashscope") => vec![("endpoint", "https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation"), ("model", "qwen-image-2.0")],
        ("multimodal", "tencent") => vec![("endpoint", "https://mps.tencentcloudapi.com"), ("model", "Hunyuan:3.0"), ("region", "ap-guangzhou")],
        ("video", "ark") => vec![("create_url", "https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks"), ("query_url", "https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks/{id}"), ("model", "doubao-seedance-2.0")],
        ("video", "dashscope") => vec![("create_url", "https://dashscope.aliyuncs.com/api/v1/services/aigc/video-generation/video-synthesis"), ("query_url", "https://dashscope.aliyuncs.com/api/v1/tasks/{id}"), ("model", "wan2.6-r2v-flash")],
        ("video", "tencent") => vec![("endpoint", "https://mps.tencentcloudapi.com"), ("create_url", "https://mps.tencentcloudapi.com"), ("query_url", "https://mps.tencentcloudapi.com"), ("model", "Hunyuan:1.5"), ("region", "ap-guangzhou")],
        ("audio", "ark") => vec![
            ("endpoint", HTTP_ENDPOINT),
            ("model", RESOURCE_ID),
        ],
        ("audio", "dashscope") => vec![("endpoint", "https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation"), ("model", "qwen3-tts-flash"), ("voice", "Cherry")],
        ("audio", "tencent") => vec![("endpoint", "https://mps.tencentcloudapi.com"), ("model", "mps-sync-dubbing"), ("region", "ap-guangzhou")],
        _ => Vec::new(),
    }
}

fn migrate_ark_plan_language_model(stored: &mut Map<String, Value>, kind: &str, provider: &str) {
    if kind == "language"
        && provider == "ark"
        && string(stored, "endpoint", "").contains("ark.cn-beijing.volces.com/api/plan/")
        && string(stored, "model", "") == "doubao-seed-1-6-250615"
    {
        stored.insert("model".to_owned(), json!("doubao-seed-2.1-turbo"));
    }
}
