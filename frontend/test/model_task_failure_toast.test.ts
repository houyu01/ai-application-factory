import assert from 'node:assert/strict';
import test from 'node:test';

import {
  notifyModelTaskFailures,
  suppressExistingModelTaskFailureNotifications,
} from '../src/model_task_failure_toast.ts';

const staleTask = {
  id: 'historical-image-failure',
  type: 'asset_image',
  status: '生成失败',
  error_message: '图片模型请求失败：HTTP 401 Unauthorized',
};

test('opening a project does not replay its historical model failures as toasts', () => {
  const messages: string[] = [];
  suppressExistingModelTaskFailureNotifications([staleTask]);
  notifyModelTaskFailures([staleTask], message => messages.push(message));

  assert.deepEqual(messages, []);
});

test('a model failure completed after the page opens still shows a toast', () => {
  const messages: string[] = [];
  notifyModelTaskFailures([
    { ...staleTask, id: 'new-image-failure', error_message: '图片模型请求失败：HTTP 429' },
  ], message => messages.push(message));

  assert.deepEqual(messages, ['素材图片生成失败：图片模型请求失败：HTTP 429']);
});
