import assert from 'node:assert/strict';
import test from 'node:test';

import { dramaShotVideoStatus, dramaVideoHistoryRecords, latestDramaVideoGeneration, latestDramaVideoUrl } from '../src/drama_video_history.ts';
import type { DramaShot } from '../src/models.ts';

function shot(overrides: Partial<DramaShot>): DramaShot {
  return { id: 'shot-1', title: '分镜', original_text: '', prompt: '', status: '生成成功', ...overrides };
}

test('defaults to the latest playable durable video version', () => {
  const item = shot({
    versions: [
      { id: 'v1', version_no: 1, status: '生成成功', video_url: '/media/one.mp4' },
      { id: 'v2', version_no: 2, status: '生成成功', video_url: '/media/two.mp4' },
    ],
  });

  assert.equal(latestDramaVideoUrl(item), '/media/two.mp4');
  assert.deepEqual(dramaVideoHistoryRecords(item).map(record => record.id), ['v2', 'v1']);
});

test('keeps the last completed video selected while a newer version is generating', () => {
  const item = shot({
    versions: [
      { id: 'v2', version_no: 2, status: '生成中', progress: 30 },
      { id: 'v1', version_no: 1, status: '生成成功', video_url: '/media/one.mp4' },
    ],
  });

  assert.equal(latestDramaVideoUrl(item), '/media/one.mp4');
});

test('shows a completed video status when a stale shot row says not generated', () => {
  const item = shot({
    status: '未生成',
    versions: [{ id: 'v1', version_no: 1, status: '生成成功', video_url: '/media/one.mp4' }],
  });

  assert.equal(dramaShotVideoStatus(item), '生成成功');
});

test('does not mistake a non-video shot failure for a video failure', () => {
  const item = shot({ status: '生成失败' });

  assert.equal(dramaShotVideoStatus(item), '未生成');
});

test('shows an active video task before its version refreshes', () => {
  const item = shot({ status: '未生成' });

  assert.equal(dramaShotVideoStatus(item, { status: '生成中' }), '生成中');
});

test('shows a video failure only when a failed video version exists', () => {
  const item = shot({
    status: '生成失败',
    versions: [{ id: 'v1', version_no: 1, status: '生成失败', error_message: '服务商拒绝请求' }],
  });

  assert.equal(dramaShotVideoStatus(item), '生成失败');
});

test('uses the newest version state instead of an older failure', () => {
  const item = shot({
    versions: [
      { id: 'v3', version_no: 3, status: '生成中', task_id: 'task-v3' },
      { id: 'v2', version_no: 2, status: '生成成功', video_url: '/media/two.mp4' },
      { id: 'v1', version_no: 1, status: '生成失败', error_message: '旧的失败信息' },
    ],
  });

  assert.equal(dramaShotVideoStatus(item), '生成中');
  assert.equal(latestDramaVideoGeneration(item)?.error, undefined);
});

test('keeps the newest successful result ahead of an older failure', () => {
  const item = shot({
    versions: [
      { id: 'v2', version_no: 2, status: '生成成功', video_url: '/media/two.mp4' },
      { id: 'v1', version_no: 1, status: '生成失败', error_message: '旧的失败信息' },
    ],
  });

  assert.equal(dramaShotVideoStatus(item), '生成成功');
  assert.equal(latestDramaVideoGeneration(item)?.status, '生成成功');
});

test('uses the most recently appended playable legacy record when no versions exist', () => {
  const item = shot({
    historical_videos: [
      { id: 'old', url: '/media/old.mp4' },
      { id: 'new', url: '/media/new.mp4' },
    ],
  });

  assert.equal(latestDramaVideoUrl(item), '/media/new.mp4');
});

test('exposes saved feedback from the selected historical version to the refinement dialog', () => {
  const item = shot({
    versions: [{ id: 'v1', version_no: 1, status: '生成成功', video_url: '/media/one.mp4', refinement_prompt: '让人物的表情更克制' }],
  });

  assert.equal(dramaVideoHistoryRecords(item)[0]?.refinementPrompt, '让人物的表情更克制');
});
