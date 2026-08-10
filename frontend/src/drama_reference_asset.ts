/** Resolves a rich-prompt reference to either a base asset or one exact character form. */
import type { DramaAsset, DramaAssetVariant, DramaPromptAssetType, DramaPromptNode } from './models.js';

type ReferenceNode = Extract<DramaPromptNode, { type: 'reference' }>;
export type DramaReferenceResource = (DramaAsset | DramaAssetVariant) & {
  type: DramaPromptAssetType;
  parent_asset_id?: string;
  variant_id?: string | null;
};

export function dramaReferenceKey(reference: Pick<ReferenceNode, 'asset_id' | 'variant_id'>) {
  return reference.variant_id ? `${reference.asset_id}:${reference.variant_id}` : reference.asset_id;
}

export function dramaReferenceAsset(assets: readonly DramaAsset[], reference: Pick<ReferenceNode, 'asset_id' | 'variant_id'>): DramaReferenceResource | undefined {
  const asset = assets.find(item => item.id === reference.asset_id);
  if (!asset) return undefined;
  if (!reference.variant_id) return asset as DramaReferenceResource;
  const variant = asset.variants?.find(item => item.id === reference.variant_id);
  if (!variant) return undefined;
  return { ...variant, type: asset.type as DramaPromptAssetType, name: `${asset.name} · ${variant.name}`, parent_asset_id: asset.id, variant_id: variant.id };
}

export function dramaReferenceOptions(assets: readonly DramaAsset[], type: DramaPromptAssetType) {
  return assets.filter(asset => asset.type === type).flatMap(asset => {
    const base: ReferenceNode = { type: 'reference', asset_id: asset.id, asset_type: type, label: asset.name, image_url: asset.image_url || null };
    if (type !== 'character') return [{ key: dramaReferenceKey(base), node: base, asset: asset as DramaReferenceResource }];
    return [
      { key: dramaReferenceKey(base), node: base, asset: asset as DramaReferenceResource },
      ...(asset.variants || []).map(variant => {
        const node: ReferenceNode = { type: 'reference', asset_id: asset.id, variant_id: variant.id, asset_type: 'character', label: `${asset.name} · ${variant.name}`, image_url: variant.image_url || null };
        return { key: dramaReferenceKey(node), node, asset: dramaReferenceAsset(assets, node)! };
      }),
    ];
  });
}
