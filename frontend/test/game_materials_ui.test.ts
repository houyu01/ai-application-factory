import assert from 'node:assert/strict';
import test from 'node:test';

import { applyQueuedGameImageTask } from '../src/game_asset_image_task_state.ts';
import { closeGameEditorPanels } from '../src/game_asset_drawer_cleanup.ts';
import { gameAssetPublicPromptDefault } from '../src/game_asset_public_prompt.ts';
import { restoreGameAssetDrawerScroll } from '../src/game_asset_drawer_scroll.ts';
import { gameMaterialRailMarkup } from '../src/game_material_rail.ts';
import { gameNodeDurationOptions } from '../src/game_node_duration.ts';
import { gameRelatedVideoFrameChoices } from '../src/game_upstream_frame_choices.ts';
import type { Game } from '../src/models.ts';

test('the game material rail exposes the full drama-style material catalog', () => {
  const markup = gameMaterialRailMarkup();

  ['character', 'scene', 'prop', 'frames', 'placeholder', 'cover'].forEach(kind => {
    assert.match(markup, new RegExp(`data-game-open-material="${kind}"`));
  });
  assert.match(markup, /drama-asset-rail/);
  assert.match(markup, /首尾帧/);
});

test('each game material type has a style-aware default public prompt', () => {
  const game = { style: '2D动漫风' } as Game;

  (['character', 'scene', 'prop'] as const).forEach(kind => {
    assert.match(gameAssetPublicPromptDefault(game, kind), /图片风格为「2D动漫风」/);
  });
  assert.match(gameAssetPublicPromptDefault(game, 'character'), /三视图/);
  assert.match(gameAssetPublicPromptDefault(game, 'character'), /六个等尺寸的表情/);
  assert.match(gameAssetPublicPromptDefault(game, 'character'), /第三排四个全身动作/);
});

test('game node duration uses five through ten second choices', () => {
  const options = gameNodeDurationOptions(15);

  assert.match(options, /<option value="5">5 秒<\/option>/);
  assert.match(options, /<option value="10" selected>10 秒<\/option>/);
  assert.doesNotMatch(options, /value="11"/);
});

test('game frame picker offers first and last frames from upstream and downstream video versions', () => {
  const game = {
    nodes: [
      { id: 'earlier', title: '更早节点', video_history: [{ id: 'v0', url: '/media/earlier.mp4', status: '生成成功' }] },
      { id: 'start', title: '入口', video_history: [{ id: 'v1', url: '/media/start.mp4', status: '生成成功' }] },
      { id: 'target', title: '调查路径', video_history: [] },
      { id: 'ending', title: '结局', video_history: [{ id: 'v2', url: '/media/ending.mp4', status: '生成成功' }] },
      { id: 'other', title: '无关节点', video_history: [{ id: 'v3', url: '/media/other.mp4', status: '生成成功' }] },
    ],
    edges: [
      { id: 'before', source_node_id: 'earlier', target_node_id: 'start', option_text: '抵达入口', sort_order: 1 },
      { id: 'edge', source_node_id: 'start', target_node_id: 'target', option_text: '继续', sort_order: 1 },
      { id: 'after', source_node_id: 'target', target_node_id: 'ending', option_text: '结局', sort_order: 1 },
    ],
  } as unknown as Game;

  const choices = gameRelatedVideoFrameChoices(game, game.nodes![2]);

  assert.deepEqual(choices.map(choice => [choice.nodeId, choice.videoId, choice.position, choice.relation]), [
    ['earlier', 'v0', 'first', 'upstream'],
    ['earlier', 'v0', 'last', 'upstream'],
    ['start', 'v1', 'first', 'upstream'],
    ['start', 'v1', 'last', 'upstream'],
    ['ending', 'v2', 'first', 'downstream'],
    ['ending', 'v2', 'last', 'downstream'],
  ]);
});

test('game asset drawer keeps its position while generation re-renders it', () => {
  const drawer = { scrollTop: 0 };

  restoreGameAssetDrawerScroll(1248, drawer);

  assert.equal(drawer.scrollTop, 1248);
});


test('queueing a material image keeps its loading state available to the open drawer', () => {
  const game = {
    id: 'game-1',
    assets: [{ id: 'hero', type: 'character', name: '林岩', prompt: '', status: '未生成', variants: [{ id: 'hero-battle', name: '战斗形态', prompt: '', status: '未生成' }] }],
    tasks: [],
  } as unknown as Game;

  applyQueuedGameImageTask(game, { id: 'task-base', type: 'game_asset_image', status: '生成中', game_id: game.id, resource_id: 'hero' });
  applyQueuedGameImageTask(game, { id: 'task-variant', type: 'game_asset_variant_image', status: '生成中', game_id: game.id, resource_id: 'hero-battle' });

  assert.equal(game.assets?.[0].status, '生成中');
  assert.equal(game.assets?.[0].variants?.[0].status, '生成中');
  assert.equal(game.tasks?.length, 2);
});

test('leaving the game editor removes detached material, cover, and placeholder panels', () => {
  const selectors: string[] = [];
  const removed: string[] = [];

  closeGameEditorPanels({
    querySelector(value) {
      selectors.push(value);
      return { remove: () => { removed.push(value); } };
    },
  });

  assert.deepEqual(selectors, ['[data-game-material-sheet]', '[data-game-cover-backdrop]', '.game-cover-picker-backdrop', '.game-placeholder-backdrop']);
  assert.deepEqual(removed, selectors);
});
