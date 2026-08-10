import type { DramaAsset, DramaPromptNode, GenerationTask } from './models.js';
import { dramaAssetImageIsGenerating, dramaImageLoadingMarkup } from './drama_asset_image_state_ui.js';
import { dramaReferenceAsset } from './drama_reference_asset.js';

type DramaReferenceNode = Extract<DramaPromptNode, { type: 'reference' }>;

type RenderOptions = {
  shotId: string;
  references: DramaReferenceNode[];
  assets: Map<string, DramaAsset>;
  tasks: GenerationTask[];
  escapeHtml: (value: unknown) => string;
  resolveMediaUrl: (value?: string | null) => string;
  kindLabel: (kind: string) => string;
  trashIcon: string;
  showStatus: boolean;
};

/** Renders current-shot references and exposes controls that never delete source assets. */
export function renderDramaReferenceCards(options: RenderOptions): string {
  const { shotId, references, assets, tasks, escapeHtml, resolveMediaUrl, kindLabel, trashIcon, showStatus } = options;
  if (!references.length) return '<div class="drama-reference-empty">暂无参考素材</div>';
  return references.map(reference => {
    const asset = dramaReferenceAsset([...assets.values()], reference);
    const label = asset?.name || reference.label || '未命名素材';
    const imageUrl = resolveMediaUrl(asset?.image_url || reference.image_url);
    const typeLabel = kindLabel(reference.asset_type);
    const generating = dramaAssetImageIsGenerating(asset, tasks);
    const missing = !asset || asset.status !== '生成成功' || !asset.image_url;
    const image = generating
      ? dramaImageLoadingMarkup(label, escapeHtml)
      : imageUrl
      ? `<img src="${escapeHtml(imageUrl)}" data-drama-image-preview="${escapeHtml(imageUrl)}" data-drama-image-label="${escapeHtml(label)}" alt="" />`
      : `<span class="drama-reference-no-image">${typeLabel === '占位图' ? '占位图' : '暂无图片'}</span>`;
    const status = showStatus ? ` · ${generating ? '生成中' : asset?.status || '素材不存在'}` : '';
    return `<div class="drama-reference-card ${missing ? 'is-missing' : ''}">
      <span class="drama-reference-thumb">${image}</span>
      <span class="drama-reference-card-copy"><b>${escapeHtml(label)}</b><small>${escapeHtml(typeLabel + status)}</small></span>
      <button type="button" class="drama-reference-remove" data-drama-remove-reference data-drama-shot-id="${escapeHtml(shotId)}" data-drama-reference-asset-id="${escapeHtml(reference.asset_id)}" data-drama-reference-variant-id="${escapeHtml(reference.variant_id || '')}" title="移除该参考图" aria-label="移除${escapeHtml(label)}参考图">${trashIcon}</button>
    </div>`;
  }).join('');
}
