import assert from 'node:assert/strict';
import test from 'node:test';

import { gamePromptNodes, gamePromptReferenceAssetIds, gamePromptReferenceOptions, isGamePromptEditorDeletionKey, serializeGamePromptNodes } from '../src/game_prompt_rich.ts';
import { gameReferencePanelMarkup } from '../src/game_reference_picker.ts';
import type { Game, GameNode } from '../src/models.ts';

test('game rich prompts preserve @ chip placement and provider-facing text', () => {
  const game = {
    id: 'game-1',
    assets: [{ id: 'dock', game_id: 'game-1', type: 'scene', name: '雾港码头', prompt: '', image_url: 'https://example.com/dock.png', status: '已配置' }],
  } as Game;
  const node = { prompt: '场景：@图1（雾港码头）', prompt_rich: [
    { type: 'text' as const, text: '场景：' },
    { type: 'reference' as const, asset_id: 'dock', asset_type: 'scene' as const, label: '旧名称' },
  ] } as GameNode;

  const serialized = serializeGamePromptNodes(game, gamePromptNodes(node));

  assert.equal(serialized.prompt, '场景：@图1（雾港码头）');
  assert.equal(serialized.nodes[1].type, 'reference');
  assert.equal((serialized.nodes[1] as { label: string }).label, '雾港码头');
  assert.deepEqual(gamePromptReferenceAssetIds(serialized.nodes), ['dock']);
});

test('game reference panel uses an add button and horizontal card list', () => {
  const game = { id: 'game-1', assets: [{ id: 'hero', game_id: 'game-1', type: 'character', name: '林砚', prompt: '', status: '已配置' }] } as Game;
  const markup = gameReferencePanelMarkup(game, ['hero'], { escapeHtml: String, resolveMediaUrl: value => value || '' });

  assert.match(markup, /data-game-add-reference/);
  assert.match(markup, /game-reference-scroll/);
  assert.match(markup, /林砚/);
});

test('game reference panel exposes one-click generation only for missing reusable materials', () => {
  const game = { id: 'game-1', assets: [
    { id: 'hero', game_id: 'game-1', type: 'character', name: '林砚', prompt: '', status: '未生成' },
    { id: 'dock', game_id: 'game-1', type: 'scene', name: '雾港码头', prompt: '', image_url: 'media://dock', status: '生成成功' },
  ] } as Game;

  const markup = gameReferencePanelMarkup(game, ['hero', 'dock'], { escapeHtml: String, resolveMediaUrl: value => value || '' });

  assert.match(markup, /有 1 个参考素材不可用/);
  assert.match(markup, /data-game-generate-reference-images/);
  assert.match(markup, /一键生成参考图/);
});

test('legacy game prompt references become the same protected chips used by the rich editor', () => {
  const game = { id: 'game-1', assets: [{ id: 'hero', game_id: 'game-1', type: 'character', name: '林砚', prompt: '', status: '已配置' }] } as Game;
  const node = { prompt: '角色：@图1（林砚）向前走。' } as GameNode;

  const nodes = gamePromptNodes(node, game);

  assert.equal(nodes[1].type, 'reference');
  assert.equal((nodes[1] as { asset_id: string }).asset_id, 'hero');
  assert.deepEqual(gamePromptReferenceOptions(game).map(item => item.label), ['林砚']);
});

test('game prompt deletion keys remain owned by the rich editor', () => {
  assert.equal(isGamePromptEditorDeletionKey('Delete'), true);
  assert.equal(isGamePromptEditorDeletionKey('Backspace'), true);
  assert.equal(isGamePromptEditorDeletionKey('Enter'), false);
});

test('game reference controls exclude assets belonging to another project', () => {
  const game = { id: 'game-1', assets: [
    { id: 'mine', game_id: 'game-1', type: 'scene', name: '游戏码头', prompt: '', status: '已配置' },
    { id: 'foreign', game_id: 'drama-1', type: 'character', name: '短剧角色', prompt: '', status: '已配置' },
  ] } as Game;

  const markup = gameReferencePanelMarkup(game, ['mine', 'foreign'], { escapeHtml: String, resolveMediaUrl: value => value || '' });

  assert.match(markup, /游戏码头/);
  assert.doesNotMatch(markup, /短剧角色/);
  assert.deepEqual(gamePromptReferenceOptions(game).map(item => item.label), ['游戏码头']);
});
