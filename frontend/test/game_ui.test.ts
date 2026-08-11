import assert from 'node:assert/strict';
import test from 'node:test';

import { restoreGameEditorScroll } from '../src/game_scroll_restore.ts';
import { gamePlayerDebugPath, gamePlayerMarkup } from '../src/game_player_ui.ts';

test('game workbench refresh restores the current main-pane scroll position', () => {
  const pane = { scrollTop: 0 };

  restoreGameEditorScroll(684, pane);

  assert.equal(pane.scrollTop, 684);
});

test('game workbench refresh restores the selected inspector scroll position', () => {
  const inspector = { scrollTop: 0 };

  restoreGameEditorScroll(932, inspector);

  assert.equal(inspector.scrollTop, 932);
});

test('game workbench refresh skips a missing main pane', () => {
  assert.doesNotThrow(() => restoreGameEditorScroll(684, null));
});

test('game player keeps choices hidden inside an active video until it ends', () => {
  const markup = gamePlayerMarkup({
    game: { id: 'game-1', name: '试玩', nodes: [], edges: [] } as never,
    session: { id: 'session-1', game_id: 'game-1', current_node_id: 'node-1', status: 'active', path: [], current_node: { id: 'node-1', title: '起点' } as never, choices: [{ id: 'edge-1', option_text: '继续', source_node_id: 'node-1', target_node_id: 'node-2', sort_order: 1 }] },
    video: 'https://example.test/node.mp4',
    escapeHtml: value => String(value),
  });

  assert.match(markup, /data-game-player-video/);
  assert.match(markup, /class="ghost game-player-restart"/);
  assert.match(markup, /data-game-player-choice-panel hidden/);
  assert.match(markup, /game-player-video-wrap[^]*game-player-choice-panel/);
  assert.doesNotMatch(markup, /当前路径|请选择接下来的行动|<b>A<\/b>/);
  assert.match(markup, /game-player-stage[^]*<\/section><p class="game-player-debug-path">/);
  assert.doesNotMatch(markup, /is-stacked/);
});

test('game player stacks options when one label is too long for a two-column layout', () => {
  const markup = gamePlayerMarkup({
    game: { id: 'game-1', name: '试玩', nodes: [], edges: [] } as never,
    session: { id: 'session-1', game_id: 'game-1', current_node_id: 'node-1', status: 'active', path: [], current_node: { id: 'node-1', title: '起点' } as never, choices: [{ id: 'edge-1', option_text: '带着刚刚收集到的线索前往远处村庄继续调查真相', source_node_id: 'node-1', target_node_id: 'node-2', sort_order: 1 }] },
    video: 'https://example.test/node.mp4',
    escapeHtml: value => String(value),
  });

  assert.match(markup, /game-player-choices is-stacked/);
});

test('game player rebuilds the full video-node and choice route for developer preview', () => {
  const path = gamePlayerDebugPath({
    nodes: [{ id: 'A', title: '村口' }, { id: 'B', title: '旧屋' }, { id: 'F', title: '线索汇合' }, { id: 'H', title: '真相揭晓' }],
    edges: [
      { id: 'AB', source_node_id: 'A', target_node_id: 'B' },
      { id: 'BF', source_node_id: 'B', target_node_id: 'F' },
      { id: 'FH', source_node_id: 'F', target_node_id: 'H' },
    ],
  } as never, {
    id: 'session-1', game_id: 'game-1', current_node_id: 'H', status: 'completed',
    path: [{ edge_id: 'AB', option_text: '进入旧屋' }, { edge_id: 'BF', option_text: '带着线索离开' }, { edge_id: 'FH', option_text: '揭开真相' }],
    current_node: { id: 'H', title: '终局' } as never, choices: [],
  });

  assert.equal(path, '村口 → 进入旧屋 → 旧屋 → 带着线索离开 → 线索汇合 → 揭开真相 → 真相揭晓');
});

test('game player makes a no-video node immediately playable', () => {
  const markup = gamePlayerMarkup({
    game: { id: 'game-1', name: '试玩', nodes: [], edges: [] } as never,
    session: { id: 'session-1', game_id: 'game-1', current_node_id: 'node-1', status: 'active', path: [], current_node: { id: 'node-1', title: '起点' } as never, choices: [] },
    video: '',
    escapeHtml: value => String(value),
  });

  assert.doesNotMatch(markup, /data-game-player-video/);
  assert.match(markup, /data-game-player-choice-panel hidden/);
});
