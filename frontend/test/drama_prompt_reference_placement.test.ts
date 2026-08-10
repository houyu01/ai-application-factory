import assert from 'node:assert/strict';
import test from 'node:test';

import { placeTrailingDramaReferences } from '../src/drama_prompt_reference_placement.ts';

test('moves legacy trailing references into their corresponding prompt fields', () => {
  const nodes = [
    { type: 'text' as const, text: '场景： @\n角色：\n风格：真人风格\n【配音：旁白】\n自动匹配参考图：' },
    { type: 'reference' as const, asset_id: 'scene', asset_type: 'scene' as const, label: '旧居' },
    { type: 'text' as const, text: '、' },
    { type: 'reference' as const, asset_id: 'character', asset_type: 'character' as const, label: '林岩' },
    { type: 'text' as const, text: '、' },
    { type: 'reference' as const, asset_id: 'prop', asset_type: 'prop' as const, label: '血泊里的碎片' },
  ];

  const repaired = placeTrailingDramaReferences(nodes);
  const prompt = repaired.map(node => node.type === 'text' ? node.text : `@${node.asset_id}`).join('');

  assert.equal(prompt, '场景：@scene\n角色：@character\n道具：@prop\n风格：真人风格\n【配音：旁白】');
  assert.equal(repaired.filter(node => node.type === 'reference').length, 3);
});

test('does not move references intentionally placed before the voice block', () => {
  const nodes = [
    { type: 'text' as const, text: '场景：' },
    { type: 'reference' as const, asset_id: 'scene', asset_type: 'scene' as const, label: '旧居' },
    { type: 'text' as const, text: '\n【配音：旁白】' },
  ];

  assert.deepEqual(placeTrailingDramaReferences(nodes), nodes);
});
