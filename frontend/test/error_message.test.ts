import assert from 'node:assert/strict';
import test from 'node:test';

import { isErrorMessage, toastDuration, translateErrorMessage } from '../src/error_message.ts';

test('translates Ark reference-image privacy errors before showing a toast', () => {
  const message = translateErrorMessage('分镜视频生成失败：Ark 视频服务失败：HTTP 400 Bad Request 原始响应：{"error":{"code":"InputImageSensitiveContentDetected.PrivacyInformation","message":"The input image may contain a real person."}}');

  assert.equal(message, '分镜视频生成失败：检测到输入图片可能包含真人或个人隐私信息，服务商拒绝生成。请替换为不含真人或隐私信息的图片后重试。');
});

test('keeps failures visible for eight seconds and marks them as errors', () => {
  assert.equal(isErrorMessage('图片生成失败：服务商暂时不可用'), true);
  assert.equal(toastDuration('图片生成失败：服务商暂时不可用'), 8_000);
  assert.equal(toastDuration('图片任务已创建'), 2_600);
});
