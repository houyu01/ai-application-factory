//! Provider-specific result classification for settings probes.

use crate::error::AppError;

/// Recognize Ark responses that prove its video task endpoint and credentials were reached.
///
/// The settings flow calls this after Ark has rejected a disposable probe task during its
/// model-specific validation. Authentication and route failures are deliberately not accepted.
pub(super) fn ark_video_probe_is_reachable(error: &AppError) -> bool {
    let message = error.to_string();
    [
        "400 Bad Request",
        "409 Conflict",
        "422 Unprocessable Entity",
        "429 Too Many Requests",
        "请求参数不符合服务商要求",
        "请求与当前任务状态冲突",
        "提交内容未通过服务商校验",
        "请求过于频繁或账户额度不足",
    ]
    .into_iter()
    .any(|status| message.contains(status))
}

#[cfg(test)]
mod tests {
    use crate::error::AppError;

    use super::ark_video_probe_is_reachable;

    #[test]
    fn ark_validation_errors_prove_the_video_probe_reached_the_service() {
        let error = AppError::External(
            "Ark 视频请求失败： HTTP status client error (400 Bad Request)".to_owned(),
        );

        assert!(ark_video_probe_is_reachable(&error));
        assert!(!ark_video_probe_is_reachable(&AppError::External(
            "Ark 视频请求失败： HTTP status client error (401 Unauthorized)".to_owned(),
        )));
        assert!(ark_video_probe_is_reachable(&AppError::External(
            "Ark 视频服务失败：请求参数不符合服务商要求，请检查模型、提示词和参考素材后重试。"
                .to_owned(),
        )));
    }
}
