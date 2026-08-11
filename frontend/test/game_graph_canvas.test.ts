import assert from 'node:assert/strict';
import test from 'node:test';

import { gameGraphCanvasMarkup, gameGraphGridStyle, gameNodeTaskIsGenerating } from '../src/game_graph_canvas.ts';

test('graph grid follows canvas pan and zoom instead of the finite graph stage', () => {
  assert.deepEqual(gameGraphGridStyle(.5, -120, 48), { size: '12px', x: '-120px', y: '48px' });
});

test('graph toolbar offers a focus overlay without rendering another graph', () => {
  const markup = gameGraphCanvasMarkup({ id: 'game-1', nodes: [], edges: [] } as never, String);
  assert.match(markup, /data-game-expand-canvas/);
  assert.match(markup, /game-graph-toolbar-actions/);
  assert.match(markup, /全屏画布/);
});

test('graph keeps negative node coordinates inside a translated large stage', () => {
  const markup = gameGraphCanvasMarkup({
    id: 'game-negative-node',
    nodes: [{ id: 'north', node_type: 'normal', title: '上方节点', original_text: '', prompt: '', duration_seconds: 10, status: '未生成', position_x: -240, position_y: -120 }],
    edges: [],
  } as never, String);
  assert.match(markup, /data-game-graph-origin="100000"/);
  assert.match(markup, /viewBox="-100000 -100000 200000 200000"/);
  assert.match(markup, /left:99760px;top:99880px/);
});

test('only nodes with active prompt or video tasks show the canvas loading icon', () => {
  const game = {
    id: 'game-video-loading',
    nodes: [
      { id: 'running', node_type: 'normal', title: '生成中节点', original_text: '', prompt: '', duration_seconds: 10, status: '未生成', position_x: 0, position_y: 0 },
      { id: 'prompt', node_type: 'normal', title: '提示词生成中节点', original_text: '', prompt: '', duration_seconds: 10, status: '未生成', position_x: 220, position_y: 0 },
      { id: 'stale', node_type: 'normal', title: '旧状态节点', original_text: '', prompt: '', duration_seconds: 10, status: '生成中', position_x: 440, position_y: 0 },
    ],
    tasks: [
      { id: 'video-task', game_id: 'game-video-loading', type: 'node_video_generation', resource_id: 'running', status: '生成中' },
      { id: 'prompt-task', game_id: 'game-video-loading', type: 'game_node_prompt', resource_id: 'prompt', status: '生成中' },
    ],
    edges: [],
  } as never;
  const markup = gameGraphCanvasMarkup(game, String);

  assert.equal(gameNodeTaskIsGenerating(game, game.nodes[0]), true);
  assert.equal(gameNodeTaskIsGenerating(game, game.nodes[1]), true);
  assert.equal(gameNodeTaskIsGenerating(game, game.nodes[2]), false);
  assert.match(markup, /data-game-node="running"[^>]*aria-busy="true"/);
  assert.match(markup, /data-game-node="running"[\s\S]*?data-game-node-loading aria-hidden="true"><span class="generation-spinner"><\/span>/);
  assert.match(markup, /data-game-node="prompt"[^>]*aria-busy="true"/);
  assert.match(markup, /data-game-node="stale"[\s\S]*?data-game-node-loading aria-hidden="true" hidden>/);
});
