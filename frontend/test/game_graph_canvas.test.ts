import assert from 'node:assert/strict';
import test from 'node:test';

import { gameGraphCanvasMarkup, gameGraphGridStyle } from '../src/game_graph_canvas.ts';

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
