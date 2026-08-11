import assert from 'node:assert/strict';
import test from 'node:test';

import { gameNodeVideoHistoryRecords, gameNodeVideoHistoryTime, selectGameNodeVideoUrl, selectedGameNodeVideoId, selectedGameNodeVideoUrl } from '../src/game_node_video_history.ts';
import type { GameNode } from '../src/models.ts';

function node(): GameNode {
  return {
    id: 'node-video-history-test', node_type: 'normal', title: '节点', original_text: '', prompt: '雨夜街道', duration_seconds: 10,
    status: '生成成功', position_x: 0, position_y: 0,
    video_url: '/media/latest.mp4',
    video_history: [
      { id: 'v1', url: '/media/first.mp4', status: '生成成功', generated_at: '2026-08-01T00:00:00Z' },
      { id: 'v2', url: '/media/latest.mp4', status: '生成成功', generated_at: '2026-08-02T00:00:00Z' },
    ],
  };
}

test('game node history adds the durable generating task before its terminal record exists', () => {
  const records = gameNodeVideoHistoryRecords(node(), {
    id: 'task-3', type: 'node_video_generation', status: '生成中', game_id: 'game-1', resource_id: 'node-video-history-test', progress: 42, created_at: '2026-08-03T00:00:00Z',
  });
  assert.equal(records[0].id, 'task-3');
  assert.equal(records[0].status, '生成中');
  assert.equal(records[0].progress, 42);
});

test('game node history renders ISO timestamps as a human-readable second-level time', () => {
  assert.equal(gameNodeVideoHistoryTime('2026-08-11T13:34:28.734585Z'), '2026-08-11 13:34:28');
});

test('game node history keeps a creator-selected previous version in the preview', () => {
  const current = node();
  selectGameNodeVideoUrl(current, '/media/first.mp4');
  assert.equal(selectedGameNodeVideoUrl(current), '/media/first.mp4');
});

test('game node history restores the durable current version after a refresh', () => {
  const current = { ...node(), id: 'node-video-history-durable-test', selected_video_id: 'v1' };
  assert.equal(selectedGameNodeVideoId(current), 'v1');
  assert.equal(selectedGameNodeVideoUrl(current), '/media/first.mp4');
});
