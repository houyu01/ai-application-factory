//! Ark, DashScope, and Tencent MPS asynchronous video protocol adapters.

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    providers::{model_for, ProviderClient, VideoJob},
};

#[path = "providers_video_prompt.rs"]
mod video_prompt;
#[path = "providers_video_state.rs"]
mod video_state;

use video_prompt::{dashscope_prompt, dashscope_reference_limit, dashscope_reference_mode};
use video_state::{
    api_key, progress, task_error, task_id, task_status, task_url, unique, video_json_response,
    video_request_error, video_task_response_error, video_url,
};

impl ProviderClient {
    /// Submit a provider video task and preserve an external task id instead of blocking the local worker.
    pub fn start_video(
        &self,
        prompt: &str,
        ratio: &str,
        resolution: &str,
        seconds: i64,
        reference_images: &[String],
        reference_video: Option<&str>,
        selected_model: Option<&str>,
    ) -> AppResult<VideoJob> {
        let config = self.config("video")?;
        self.start_video_with_config(
            &config,
            prompt,
            ratio,
            resolution,
            seconds,
            reference_images,
            reference_video,
            selected_model,
        )
    }

    /// Submit a video task from an unpersisted settings candidate during the real configuration probe.
    pub(crate) fn start_video_with_config(
        &self,
        config: &Map<String, Value>,
        prompt: &str,
        ratio: &str,
        resolution: &str,
        seconds: i64,
        reference_images: &[String],
        reference_video: Option<&str>,
        selected_model: Option<&str>,
    ) -> AppResult<VideoJob> {
        let provider = config
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("ark");
        let model = model_for(config, selected_model);
        if model.is_empty() {
            return Err(AppError::BadRequest("视频模型尚未配置模型名称".to_owned()));
        }
        match provider {
            "dashscope" => self.start_dashscope(
                config,
                &model,
                prompt,
                ratio,
                resolution,
                seconds,
                reference_images,
                reference_video,
            ),
            "tencent" => self.start_tencent(
                config,
                &model,
                prompt,
                ratio,
                resolution,
                seconds,
                reference_images,
                reference_video,
            ),
            _ => self.start_ark(
                config,
                &model,
                prompt,
                ratio,
                resolution,
                seconds,
                reference_images,
                reference_video,
            ),
        }
    }

    /// Poll one previously persisted provider task and return a media URL only after it is complete.
    pub fn poll_video(&self, provider_task_id: &str) -> AppResult<VideoJob> {
        let config = self.config("video")?;
        self.poll_video_with_config(&config, provider_task_id)
    }

    /// Poll a task from an unpersisted model-settings candidate during credential verification.
    pub(crate) fn poll_video_with_config(
        &self,
        config: &Map<String, Value>,
        provider_task_id: &str,
    ) -> AppResult<VideoJob> {
        let provider = config
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("ark");
        match provider {
            "dashscope" => self.poll_dashscope(config, provider_task_id),
            "tencent" => self.poll_tencent(config, provider_task_id),
            _ => self.poll_ark(config, provider_task_id),
        }
    }

    /// Best-effort remote cancellation used after local durable task rows are stopped.
    pub fn cancel_video(&self, provider_task_id: &str) -> AppResult<()> {
        let config = self.config("video")?;
        self.cancel_video_with_config(&config, provider_task_id)
    }

    /// Best-effort cancellation for a video configuration probe that has not been stored yet.
    pub(crate) fn cancel_video_with_config(
        &self,
        config: &Map<String, Value>,
        provider_task_id: &str,
    ) -> AppResult<()> {
        let provider = config
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("ark");
        if provider == "tencent" {
            return Ok(());
        }
        let key = api_key(config)?;
        let url = if provider == "dashscope" {
            format!(
                "{}/cancel",
                task_url(
                    config
                        .get("query_url")
                        .and_then(Value::as_str)
                        .unwrap_or("https://dashscope.aliyuncs.com/api/v1/tasks/{id}"),
                    provider_task_id
                )
                .trim_end_matches('/')
            )
        } else {
            task_url(
                config
                    .get("query_url")
                    .and_then(Value::as_str)
                    .or_else(|| config.get("endpoint").and_then(Value::as_str))
                    .unwrap_or_default(),
                provider_task_id,
            )
        };
        let request = self
            .client
            .request(
                if provider == "dashscope" {
                    reqwest::Method::POST
                } else {
                    reqwest::Method::DELETE
                },
                url,
            )
            .header(AUTHORIZATION, format!("Bearer {key}"));
        let request = if provider == "dashscope" {
            request.header("X-DashScope-Async", "enable")
        } else {
            request
        };
        request
            .send()
            .map_err(video_request_error(provider))?
            .error_for_status()
            .map_err(video_request_error(provider))?;
        Ok(())
    }

    /// Verify Tencent TC3 credentials without creating a potentially billable video task.
    pub(crate) fn probe_tencent_video_credentials(
        &self,
        config: &Map<String, Value>,
    ) -> AppResult<()> {
        match self.tencent_request(
            config,
            "DescribeAigcVideoTask",
            &json!({"TaskId":"probe-invalid-task"}),
        ) {
            Ok(_) => Ok(()),
            Err(AppError::External(message))
                if message.contains("InvalidParameter")
                    || message.contains("ResourceNotFound")
                    || message.contains("FailedOperation") =>
            {
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn start_ark(
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
        let key = api_key(config)?;
        let url = config
            .get("create_url")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .or_else(|| config.get("endpoint").and_then(Value::as_str))
            .ok_or_else(|| AppError::BadRequest("视频模型尚未配置创建地址".to_owned()))?;
        let content = ark_content(prompt, references, reference_video);
        let response = video_json_response(
            "Ark",
            self.client
                .post(url)
                .header(AUTHORIZATION, format!("Bearer {key}"))
                .json(&json!({"model":model,"content":content,"generate_audio":true,"ratio":ratio,"resolution":resolution,"duration":seconds,"watermark":false}))
                .send()
                .map_err(video_request_error("Ark"))?,
        )?;
        self.read_submission(&response, "Ark")
    }

    fn start_dashscope(
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
        let key = api_key(config)?;
        let images = unique(references);
        let model_lower = model.to_lowercase();
        if let Some(video) = reference_video.filter(|value| !value.is_empty()) {
            if !dashscope_reference_mode(&model_lower) {
                return Err(AppError::BadRequest(format!(
                    "阿里云模型“{model}”不支持视频微调所需的参考视频输入。请改用 wan2.6-r2v 系列模型。"
                )));
            }
            let mut reference_urls = vec![video.to_owned()];
            reference_urls.extend(images);
            let input = json!({"prompt":dashscope_prompt(prompt, model),"reference_urls":unique(&reference_urls)});
            let parameters = json!({"size":dashscope_size(ratio, resolution),"duration":seconds,"audio":true,"shot_type":"multi"});
            return self.submit_dashscope(config, model, key, input, parameters);
        }
        let reference_mode = dashscope_reference_mode(&model_lower);
        if model.to_lowercase().ends_with("-i2v") {
            return Err(AppError::BadRequest(format!("阿里云模型“{model}”仅支持接口原生首帧图模式，本应用不使用该模式。请改用 wan2.6-r2v-flash。")));
        }
        if reference_mode
            && (images.is_empty() || images.len() > dashscope_reference_limit(&model_lower))
        {
            return Err(AppError::BadRequest(format!(
                "阿里云模型“{model}”需要传入有效数量的参考图。"
            )));
        }
        if !reference_mode && !images.is_empty() {
            return Err(AppError::BadRequest(format!(
                "阿里云模型“{model}”不支持参考图生视频。请改用支持 reference_image 的 R2V 模型。"
            )));
        }
        let input = if images.is_empty() {
            json!({"prompt":prompt})
        } else {
            json!({"prompt":dashscope_prompt(prompt, model),"media":images.into_iter().map(|url| json!({"type":"reference_image","url":url})).collect::<Vec<_>>()})
        };
        let mut parameters =
            json!({"resolution":resolution.to_uppercase(),"duration":seconds,"audio":true});
        if reference_mode {
            parameters["ratio"] = json!(ratio);
        }
        self.submit_dashscope(config, model, key, input, parameters)
    }

    /// Submit a DashScope request after the caller has chosen its model-specific reference-media shape.
    fn submit_dashscope(
        &self,
        config: &Map<String, Value>,
        model: &str,
        key: &str,
        input: Value,
        parameters: Value,
    ) -> AppResult<VideoJob> {
        let url = config.get("create_url").and_then(Value::as_str).filter(|value| !value.is_empty()).unwrap_or("https://dashscope.aliyuncs.com/api/v1/services/aigc/video-generation/video-synthesis");
        let response = video_json_response(
            "DashScope",
            self.client
                .post(url)
                .header(AUTHORIZATION, format!("Bearer {key}"))
                .header(CONTENT_TYPE, "application/json")
                .header("X-DashScope-Async", "enable")
                .json(&json!({"model":model,"input":input,"parameters":parameters}))
                .send()
                .map_err(video_request_error("DashScope"))?,
        )?;
        self.read_submission(&response, "DashScope")
    }

    fn poll_ark(&self, config: &Map<String, Value>, task_id: &str) -> AppResult<VideoJob> {
        let key = api_key(config)?;
        let template = config
            .get("query_url")
            .and_then(Value::as_str)
            .or_else(|| config.get("endpoint").and_then(Value::as_str))
            .ok_or_else(|| AppError::BadRequest("视频模型尚未配置查询地址".to_owned()))?;
        let response = video_json_response(
            "Ark",
            self.client
                .get(task_url(template, task_id))
                .header(AUTHORIZATION, format!("Bearer {key}"))
                .send()
                .map_err(video_request_error("Ark"))?,
        )?;
        self.read_poll(&response, "Ark", task_id)
    }

    fn poll_dashscope(&self, config: &Map<String, Value>, task_id: &str) -> AppResult<VideoJob> {
        let key = api_key(config)?;
        let template = config
            .get("query_url")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or("https://dashscope.aliyuncs.com/api/v1/tasks/{id}");
        let response = video_json_response(
            "DashScope",
            self.client
                .get(task_url(template, task_id))
                .header(AUTHORIZATION, format!("Bearer {key}"))
                .header("X-DashScope-Async", "enable")
                .send()
                .map_err(video_request_error("DashScope"))?,
        )?;
        self.read_poll(&response, "DashScope", task_id)
    }

    pub(super) fn read_submission(&self, response: &Value, provider: &str) -> AppResult<VideoJob> {
        if let Some(url) = video_url(response) {
            return self.media.save_url(url, ".mp4").map(VideoJob::Ready);
        }
        let id = task_id(response).ok_or_else(|| {
            video_task_response_error(provider, "视频模型没有返回任务 ID", response)
        })?;
        Ok(VideoJob::Pending {
            provider_task_id: id,
            progress: progress(response),
        })
    }

    pub(super) fn read_poll(
        &self,
        response: &Value,
        provider: &str,
        id: &str,
    ) -> AppResult<VideoJob> {
        let status = task_status(response);
        if ["succeeded", "completed", "success", "succeed", "done"].contains(&status.as_str()) {
            return video_url(response)
                .ok_or_else(|| {
                    video_task_response_error(
                        provider,
                        "视频任务已完成，但没有返回 video_url",
                        response,
                    )
                })
                .and_then(|url| self.media.save_url(url, ".mp4"))
                .map(VideoJob::Ready);
        }
        if ["failed", "fail", "canceled", "cancelled", "error"].contains(&status.as_str()) {
            return Err(video_task_response_error(
                provider,
                &format!("视频生成失败：{}", task_error(response).unwrap_or(status)),
                response,
            ));
        }
        Ok(VideoJob::Pending {
            provider_task_id: id.to_owned(),
            progress: progress(response).max(5),
        })
    }
}

fn ark_content(prompt: &str, references: &[String], reference_video: Option<&str>) -> Vec<Value> {
    let mut content = vec![json!({"type":"text","text":prompt})];
    if let Some(video) = reference_video.filter(|value| !value.is_empty()) {
        content
            .push(json!({"type":"video_url","video_url":{"url":video},"role":"reference_video"}));
    }
    for image in unique(references) {
        content
            .push(json!({"type":"image_url","image_url":{"url":image},"role":"reference_image"}));
    }
    content
}

fn dashscope_size(ratio: &str, resolution: &str) -> &'static str {
    match (ratio, resolution.to_lowercase().as_str()) {
        ("9:16", "480p") => "480*832",
        ("16:9", "480p") => "854*480",
        ("9:16", _) => "720*1280",
        _ => "1280*720",
    }
}

#[cfg(test)]
mod tests {
    use super::ark_content;

    #[test]
    fn ark_refinement_request_carries_the_source_video_and_images() {
        let content = ark_content(
            "微调提示",
            &["https://example.com/reference.png".to_owned()],
            Some("https://example.com/source.mp4"),
        );

        assert_eq!(content[1]["type"], "video_url");
        assert_eq!(
            content[1]["video_url"]["url"],
            "https://example.com/source.mp4"
        );
        assert_eq!(content[2]["role"], "reference_image");
    }
}
