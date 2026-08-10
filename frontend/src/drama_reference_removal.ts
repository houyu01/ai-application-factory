import type { DramaPromptNode } from './models.js';

/** Derives the persisted prompt references after one visible reference card is removed. */
export function removeDramaReference(nodes: DramaPromptNode[], assetId: string, variantId?: string) {
  return nodes.filter(node => node.type !== 'reference' || node.asset_id !== assetId || (node.variant_id || '') !== (variantId || ''));
}

/** Produces the deduplicated asset IDs required by video and reference-image workflows. */
export function dramaReferenceAssetIds(nodes: DramaPromptNode[]) {
  return [...new Set(nodes.flatMap(node => node.type === 'reference' ? [node.asset_id] : []))];
}
