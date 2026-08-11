import assert from 'node:assert/strict';
import test from 'node:test';

import { applyQueuedGameImageTask } from '../src/game_asset_image_task_state.ts';
import { closeGameEditorPanels } from '../src/game_asset_drawer_cleanup.ts';
import { gameAssetPublicPromptDefault } from '../src/game_asset_public_prompt.ts';
import { gameMaterialRailMarkup } from '../src/game_material_rail.ts';
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
