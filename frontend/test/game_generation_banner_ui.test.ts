import assert from 'node:assert/strict';
import test from 'node:test';

import { gameGenerationBannerMarkup, gameGenerationCopy } from '../src/game_generation_banner_ui.ts';
import type { Game } from '../src/models.ts';

function game(overrides: Partial<Game> = {}): Game {
  return {
    id: 'game-1', name: '实时分支剧本', script: '原始剧本', platform: 'Steam游戏', style: '真人风格',
    success_ending_count: 1, failure_ending_count: 1, branch_min: 2, branch_max: 2,
    node_duration_min: 5, node_duration_max: 10, language_model: 'language', multimodal_model: 'image',
    video_model: 'video', status: '生成中', nodes: [], edges: [], assets: [], tasks: [], ...overrides,
  };
}

test('game screenplay expansion exposes the persisted incremental text in the workbench banner', () => {
  const running = game({
    expanded_script: '【剧情段 S01】林默踏进雨夜的车站。',
    tasks: [{
      id: 'expand-1', type: 'game_script_expansion', status: '生成中', game_id: 'game-1', progress: 36,
      stage: '正在扩写互动游戏剧本（已接收 17 字）',
      input_snapshot: { expanded_script_preview: '【剧情段 S01】林默踏进雨夜的车站。' },
    }],
  });

  const copy = gameGenerationCopy(running)!;
  const markup = gameGenerationBannerMarkup(running, value => String(value));

  assert.equal(copy.preview, running.expanded_script);
  assert.equal(copy.receivedLabel, `剧本已接收 ${Array.from(running.expanded_script || '').length} 字`);
  assert.match(copy.title, /正在实时输出/);
  assert.match(markup, /林默踏进雨夜的车站/);
  assert.match(markup, /data-game-generation-preview/);
});

test('game graph generation retains the live skeleton summary after screenplay expansion finishes', () => {
  const copy = gameGenerationCopy(game({
    tasks: [{
      id: 'graph-1', type: 'game_graph_decomposition', status: '生成中', game_id: 'game-1', progress: 70,
      stage: '正在拆分视频节点骨架',
      input_snapshot: { graph_preview: '{"node_type":"start","source_node_id":"start"}', preview_received_chars: 44, preview_node_count: 1, preview_edge_count: 1 },
    }],
  }))!;

  assert.equal(copy.graphPlanning, true);
  assert.equal(copy.receivedLabel, '骨架已接收 44 字');
  assert.equal(copy.nodeCount, 1);
  assert.equal(copy.edgeCount, 1);
});
