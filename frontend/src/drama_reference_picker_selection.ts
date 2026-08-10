import type { DramaPromptNode } from './models.js';

type DramaReferenceNode = Extract<DramaPromptNode, { type: 'reference' }>;

function referenceKey(reference: DramaReferenceNode) {
  return reference.variant_id ? `${reference.asset_id}:${reference.variant_id}` : reference.asset_id;
}

/** Resolves every selected picker ID from the persistent cross-category option map. */
export function selectedDramaReferenceNodes(selected: Iterable<string>, nodeById: ReadonlyMap<string, DramaReferenceNode>) {
  return [...selected].flatMap(id => {
    const node = nodeById.get(id);
    return node ? [node] : [];
  });
}

/** Applies a picker selection while preserving prompt text and existing chip placement. */
export function reconcileDramaReferenceNodes(existingNodes: DramaPromptNode[], selectedNodes: DramaReferenceNode[]) {
  const selectedKeys = new Set(selectedNodes.map(referenceKey));
  const existingKeys = new Set(existingNodes.filter(node => node.type === 'reference').map(referenceKey));
  const retained = existingNodes.filter(node => node.type !== 'reference' || selectedKeys.has(referenceKey(node)));
  return [...retained, ...selectedNodes.filter(node => !existingKeys.has(referenceKey(node)))];
}
