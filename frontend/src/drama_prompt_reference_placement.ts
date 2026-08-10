import type { DramaPromptAssetType, DramaPromptNode } from './models.js';

type ReferenceNode = Extract<DramaPromptNode, { type: 'reference' }>;

const sectionLabels: Partial<Record<DramaPromptAssetType, string>> = {
  scene: '场景',
  character: '角色',
  prop: '道具',
};

function textNode(text: string): DramaPromptNode {
  return { type: 'text', text };
}

function isTrailingReference(node: DramaPromptNode, lastVoiceIndex: number, index: number) {
  return index > lastVoiceIndex && node.type === 'reference';
}

function removeMentionPlaceholder(text: string) {
  return text.replace(/^\s*@(?:图\d*)?(?:（[^）]*）|\([^)]*\))?/, '');
}

function insertAtSection(nodes: DramaPromptNode[], type: DramaPromptAssetType, references: ReferenceNode[]) {
  const label = sectionLabels[type];
  if (!label || !references.length) return nodes;
  const marker = new RegExp(`${label}[：:]`);
  const nodeIndex = nodes.findIndex(node => node.type === 'text' && marker.test(node.text));
  if (nodeIndex < 0) return insertMissingSection(nodes, label, references);
  const node = nodes[nodeIndex] as Extract<DramaPromptNode, { type: 'text' }>;
  const match = node.text.match(marker)!;
  const markerEnd = match.index! + match[0].length;
  const before = node.text.slice(0, markerEnd);
  const after = removeMentionPlaceholder(node.text.slice(markerEnd));
  const inserted: DramaPromptNode[] = [textNode(before)];
  references.forEach((reference, index) => {
    if (index) inserted.push(textNode('、'));
    inserted.push(reference);
  });
  if (after) inserted.push(textNode(after));
  return [...nodes.slice(0, nodeIndex), ...inserted, ...nodes.slice(nodeIndex + 1)];
}

function insertMissingSection(nodes: DramaPromptNode[], label: string, references: ReferenceNode[]) {
  const styleIndex = nodes.findIndex(node => node.type === 'text' && /风格[：:]/.test(node.text));
  const inserted: DramaPromptNode[] = [textNode(`\n${label}：`)];
  references.forEach((reference, index) => {
    if (index) inserted.push(textNode('、'));
    inserted.push(reference);
  });
  const position = styleIndex < 0 ? nodes.length : styleIndex;
  return [...nodes.slice(0, position), ...inserted, ...nodes.slice(position)];
}

/**
 * Repairs legacy fallback prompts that stored every reference after the voice block.
 * The legacy tail contains only generated references, so moving it is safe and keeps
 * every chip adjacent to its scene, character, or prop field.
 */
export function placeTrailingDramaReferences(nodes: DramaPromptNode[]) {
  const lastVoiceIndex = nodes.reduce((last, node, index) => node.type === 'text' && /【配音|配音[：:]/.test(node.text) ? index : last, -1);
  const references = nodes.filter((node, index): node is ReferenceNode => isTrailingReference(node, lastVoiceIndex, index));
  if (lastVoiceIndex < 0 || !references.length) return nodes;
  const head = nodes.slice(0, lastVoiceIndex + 1).map(node => node.type === 'text'
    ? textNode(node.text.replace(/\n?自动匹配参考图：\s*$/, ''))
    : node);
  const retainedTail = nodes.slice(lastVoiceIndex + 1).filter((node, index) => {
    if (node.type === 'reference') return false;
    return !/^\s*(?:自动匹配参考图：|、)?\s*$/.test(node.text) || index === 0 && !node.text.includes('自动匹配参考图');
  });
  let repaired = [...head, ...retainedTail];
  (Object.keys(sectionLabels) as DramaPromptAssetType[]).forEach(type => {
    repaired = insertAtSection(repaired, type, references.filter(reference => reference.asset_type === type));
  });
  const unmapped = references.filter(reference => !sectionLabels[reference.asset_type]);
  return unmapped.length ? [...repaired, textNode('\n参考图：'), ...unmapped] : repaired;
}
