import assert from 'node:assert/strict';
import test from 'node:test';

import { serialGameBatchProgress } from '../src/game_video_batch_state.ts';
import type { Game } from '../src/models.ts';

function game(tasks: Game['tasks'] = []): Game {
  return {
    id: 'game-1', name: '测试', script: '', platform: 'Steam游戏', style: '真人风格',
    success_ending_count: 1, failure_ending_count: 1, branch_min: 2, branch_max: 2,
    node_duration_min: 5, node_duration_max: 10, language_model: 'language',
    multimodal_model: 'image', video_model: 'video', status: '已完成', tasks,
  };
}

test('serial game batch progress reads the durable coordinator snapshot', () => {
  const progress = serialGameBatchProgress(game([{
    id: 'batch-1', game_id: 'game-1', type: 'serial_game_node_video_batch', status: '生成中',
    input_snapshot: { completed_count: 3, total_count: 8 },
  }]));

  assert.deepEqual(progress, { completed: 3, total: 8 });
});

test('serial game batch progress ignores finished coordinators', () => {
  assert.equal(serialGameBatchProgress(game([{
    id: 'batch-1', game_id: 'game-1', type: 'serial_game_node_video_batch', status: '生成成功',
  }])), null);
});
