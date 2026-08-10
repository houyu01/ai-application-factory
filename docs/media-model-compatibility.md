# 媒体模型兼容矩阵

本矩阵描述桌面工作台已实现的图像、视频与音频适配。设置保存前会执行真实连通性嗅探；无凭据的自动化测试只验证请求/返回转换，不会创建计费任务。

| 服务商 | 类别 / 默认模型 | 工作台请求适配 | 成功返回处理 |
| --- | --- | --- | --- |
| 火山引擎 Ark | 图像 / `doubao-seedream-4-0-250828` | `prompt`，可选 `image` 参考图 | `data[0].url` 或 `b64_json` 转存本地 |
| 阿里云 DashScope | 图像 / `qwen-image-2.0` | `input.messages[].content`，比例转 `size` | `output.results` 或多模态消息中的 URL 转存 |
| 腾讯云 MPS | 图像 / `Hunyuan:3.0` | TC3 签名、`ModelName` / `ModelVersion` / `Prompt` | 轮询 `DescribeAigcImageTask` 的 `Response.Status` 与 `ImageUrls`，立即转存 |
| 火山引擎 Ark | 视频 / `doubao-seedance-2.0` | `content` 中的文本、图片、视频参考项 | 任务 ID 或视频 URL；持久化后异步轮询 |
| 阿里云 DashScope | 视频 / `wan2.6-r2v-flash` | `reference_urls`、`size`、`duration`、`audio`、`shot_type` | `task_id` 后轮询，完成 URL 转存 |
| 腾讯云 MPS | 视频 / `Hunyuan:1.5` | TC3 签名、模型名/版本、图片和视频参考素材 | `TaskId` 后轮询，完成 URL 转存 |
| 火山 / DashScope / 腾讯云 | 音频 | 分别校验异步 TTS、Qwen3-TTS、SyncDubbing 的必填项及成功输出 | 设置嗅探验证音频 URL 或 Base64 输出 |

## 约束

- 腾讯云 MPS 图像当前显式支持文生图；带参考图的请求会在提交前拒绝，避免把不受当前模型协议支持的字段发送为计费任务。
- DashScope 的 R2V 模型必须至少有一项参考素材；Wan 2.6 在中国区需配置对应工作空间 Endpoint。
- 腾讯云生成结果 URL 有有效期，工作台会在任务完成后转存到本地媒体库。

## 依据

- [腾讯云 MPS AI 图片生成](https://cloud.tencent.com/document/product/862/132095)
- [腾讯云 MPS 通用创作](https://cloud.tencent.com/document/product/862/132096)
- [DashScope Wan 2.6 R2V API](https://help.aliyun.com/en/model-studio/legacy-wan-reference-to-video-api-reference)
- [DashScope Qwen-Image API](https://help.aliyun.com/zh/model-studio/qwen-image-api)
- [DashScope Qwen-TTS API](https://help.aliyun.com/en/model-studio/qwen-tts-api)
