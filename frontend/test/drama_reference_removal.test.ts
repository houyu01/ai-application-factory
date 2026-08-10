import assert from 'node:assert/strict';
import test from 'node:test';

import { dramaReferenceAssetIds, removeDramaReference } from '../src/drama_reference_removal.ts';
import { dramaReferenceAsset, dramaReferenceOptions } from '../src/drama_reference_asset.ts';
import { reconcileDramaReferenceNodes, selectedDramaReferenceNodes } from '../src/drama_reference_picker_selection.ts';
import type { DramaAsset } from '../src/models.ts';

test('removing one reference retains every other reference and prompt text', () => {
  const nodes = [
    { type: 'text' as const, text: '角色在场景中行动。' },
    { type: 'reference' as const, asset_id: 'asset-character', asset_type: 'character' as const, label: '主角' },
    { type: 'reference' as const, asset_id: 'asset-scene', asset_type: 'scene' as const, label: '客厅' },
    { type: 'reference' as const, asset_id: 'asset-prop', asset_type: 'prop' as const, label: '信件' },
  ];

  const remaining = removeDramaReference(nodes, 'asset-scene');

  assert.deepEqual(remaining, [nodes[0], nodes[1], nodes[3]]);
  assert.deepEqual(dramaReferenceAssetIds(remaining), ['asset-character', 'asset-prop']);
});

test('picker returns selections from multiple asset categories', () => {
  const nodeById = new Map([
    ['asset-character', { type: 'reference' as const, asset_id: 'asset-character', asset_type: 'character' as const, label: '主角' }],
    ['asset-scene', { type: 'reference' as const, asset_id: 'asset-scene', asset_type: 'scene' as const, label: '客厅' }],
  ]);

  assert.deepEqual(selectedDramaReferenceNodes(new Set(['asset-character', 'asset-scene']), nodeById), [...nodeById.values()]);
});

test('picker selection can replace an already-added character without losing prompt text', () => {
  const prompt = [
    { type: 'text' as const, text: '旧角色：' },
    { type: 'reference' as const, asset_id: 'old-character', asset_type: 'character' as const, label: '旧角色' },
    { type: 'text' as const, text: '在场景中行动。' },
  ];
  const selected = [{ type: 'reference' as const, asset_id: 'new-character', asset_type: 'character' as const, label: '新角色' }];

  assert.deepEqual(reconcileDramaReferenceNodes(prompt, selected), [prompt[0], prompt[2], selected[0]]);
});

test('character forms remain distinct references with their own reference image', () => {
  const character: DramaAsset = {
    id: 'lin-yan', type: 'character', name: '林砚', prompt: '成年剑修', image_url: '/adult.png', status: '生成成功',
    variants: [{ id: 'lin-yan-child', name: '幼年形态', prompt: '八岁，粗布短褂', image_url: '/child.png', status: '生成成功' }],
  };
  const child = { type: 'reference' as const, asset_id: 'lin-yan', variant_id: 'lin-yan-child', asset_type: 'character' as const, label: '林砚 · 幼年形态' };
  const resolved = dramaReferenceAsset([character], child);

  assert.equal(resolved?.id, 'lin-yan-child');
  assert.equal(resolved?.image_url, '/child.png');
  assert.deepEqual(dramaReferenceOptions([character], 'character').map(option => option.key), ['lin-yan', 'lin-yan:lin-yan-child']);
  assert.deepEqual(removeDramaReference([{ type: 'reference' as const, asset_id: 'lin-yan', asset_type: 'character' as const, label: '林砚' }, child], 'lin-yan', 'lin-yan-child'), [{ type: 'reference', asset_id: 'lin-yan', asset_type: 'character', label: '林砚' }]);
});
