import assert from 'node:assert/strict';
import test from 'node:test';

import { GAME_TASK_REFRESH_INTERVAL_MS, gameGraphSignature, gameHasRunningTasks, mergeGameTaskSnapshot } from '../src/game_task_refresh_state.ts';
import type { Game } from '../src/models.ts';

function game(overrides: Partial<Game> = {}): Game {
  return {
    id: 'game-1', name: '测试', script: '', platform: 'Steam游戏', style: '真人风格',
    success_ending_count: 1, failure_ending_count: 1, branch_min: 2, branch_max: 2,
    node_duration_min: 5, node_duration_max: 10, language_model: 'language',
    multimodal_model: 'image', video_model: 'video', status: '已完成',
    nodes: [{ id: 'node-1', node_type: 'start', title: '入口', original_text: '', prompt: '', duration_seconds: 10, status: '未生成', position_x: 0, position_y: 0 }],
    edges: [], assets: [], tasks: [], ...overrides,
  };
}

test('only generating tasks keep the background game task refresh active', () => {
  assert.equal(gameHasRunningTasks(game()), false);
  assert.equal(gameHasRunningTasks(game({ tasks: [{ id: 'task-1', type: 'node_video_generation', status: '生成中', game_id: 'game-1' }] })), true);
  assert.equal(GAME_TASK_REFRESH_INTERVAL_MS, 3_000);
});

test('task refresh keeps current node identity while updating its durable state', () => {
  const current = game();
  const selected = current.nodes![0];
  mergeGameTaskSnapshot(current, game({ nodes: [{ ...selected, status: '生成成功', video_url: '/api/media/video.mp4' }] }));

  assert.equal(current.nodes![0], selected);
  assert.equal(selected.status, '生成成功');
  assert.equal(selected.video_url, '/api/media/video.mp4');
});

test('graph signatures ignore task updates and change for new nodes or edges', () => {
  const current = game();
  assert.equal(gameGraphSignature(current), gameGraphSignature(game({ tasks: [{ id: 'task-1', type: 'node_video_generation', status: '生成中', game_id: 'game-1' }] })));
  assert.notEqual(gameGraphSignature(current), gameGraphSignature(game({ nodes: [...current.nodes!, { ...current.nodes![0], id: 'node-2' }] })));
});
