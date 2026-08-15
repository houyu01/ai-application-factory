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
  assert.ok(markup.indexOf('data-game-generation-preview') < markup.indexOf('data-game-stop-generation'));
  assert.match(markup, /已经花费0小时0分钟0秒，调用大模型生产时间可能较长/);
  assert.match(markup, /class="generation-exit-warning"[^>]*>请勿退出应用<\/span>/);
  assert.doesNotMatch(markup, /drama-decomposition-wait-notice/);
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

test('game graph validation retry stays on the save step while it regenerates only rejected edges', () => {
  const copy = gameGenerationCopy(game({
    tasks: [{
      id: 'graph-1', type: 'game_graph_decomposition', status: '生成中', game_id: 'game-1', progress: 70,
      stage: '模型图谱校验未通过，将保留已完成素材和视频节点，仅重新生成选择边，1 秒后自动重试（第 1/4 次）', input_snapshot: {},
    }],
  }))!;

  assert.equal(copy.failed, false);
  assert.equal(copy.step, 3);
  assert.match(copy.title, /第 4\/4 步：保存图谱/);
  assert.match(copy.detail, /仅重新生成选择边/);
});

test('game expansion and graph decomposition share one elapsed-time run', () => {
  const expansion = { id: 'expand-1', type: 'game_script_expansion', status: '生成成功', game_id: 'game-1', input_snapshot: {}, started_at: '2026-08-14T10:00:00Z' };
  const graph = { id: 'graph-1', type: 'game_graph_decomposition', status: '生成中', game_id: 'game-1', input_snapshot: {}, started_at: '2026-08-14T10:05:00Z' };
  const copy = gameGenerationCopy(game({ tasks: [expansion, graph] }))!;

  assert.equal(copy.timerKey, 'game:game-1:expand-1');
  assert.equal(copy.timerStartedAt, expansion.started_at);
});

test('failed game generation offers to continue instead of retry', () => {
  const markup = gameGenerationBannerMarkup(game({
    tasks: [{
      id: 'expand-failed', type: 'game_script_expansion', status: '生成失败',
      game_id: 'game-1', error_message: '连接中断', input_snapshot: {},
    }],
  }), String);

  assert.match(markup, /id="game-retry-generation">继续<\/button>/);
  assert.doesNotMatch(markup, />重试<\/button>/);
});
