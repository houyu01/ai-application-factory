import assert from 'node:assert/strict';
import test from 'node:test';

import { gamePromptNodes, gamePromptReferenceAssetIds, gamePromptReferenceOptions, serializeGamePromptNodes } from '../src/game_prompt_rich.ts';
import { gameReferencePanelMarkup } from '../src/game_reference_picker.ts';
import type { Game, GameNode } from '../src/models.ts';

test('game rich prompts preserve @ chip placement and provider-facing text', () => {
  const game = {
    assets: [{ id: 'dock', type: 'scene', name: '雾港码头', prompt: '', image_url: 'https://example.com/dock.png', status: '已配置' }],
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
  const game = { assets: [{ id: 'hero', type: 'character', name: '林砚', prompt: '', status: '已配置' }] } as Game;
  const markup = gameReferencePanelMarkup(game, ['hero'], { escapeHtml: String, resolveMediaUrl: value => value || '' });

  assert.match(markup, /data-game-add-reference/);
  assert.match(markup, /game-reference-scroll/);
  assert.match(markup, /林砚/);
});

test('legacy game prompt references become the same protected chips used by the rich editor', () => {
  const game = { assets: [{ id: 'hero', type: 'character', name: '林砚', prompt: '', status: '已配置' }] } as Game;
  const node = { prompt: '角色：@图1（林砚）向前走。' } as GameNode;

  const nodes = gamePromptNodes(node, game);

  assert.equal(nodes[1].type, 'reference');
  assert.equal((nodes[1] as { asset_id: string }).asset_id, 'hero');
  assert.deepEqual(gamePromptReferenceOptions(game).map(item => item.label), ['林砚']);
});
