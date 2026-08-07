/** Shared image-state helpers keep every reference of a generating asset in sync. */
import type { DramaAsset, DramaAssetVariant, GenerationTask } from './models.js';

type ImageResource = DramaAsset | DramaAssetVariant;

const imageTaskTypes = new Set(['asset_image', 'asset_variant_image', 'placeholder_image', 'cover_image']);
const imageBatchTaskTypes = new Set(['asset_image_batch', 'shot_reference_image_batch']);

function taskTargetsImage(task: GenerationTask, assetId: string) {
  if (imageTaskTypes.has(task.type)) return task.resource_id === assetId;
  const jobs = task.input_snapshot?.jobs;
  return imageBatchTaskTypes.has(task.type) && Array.isArray(jobs) && jobs.some(job => typeof job === 'object' && job !== null && (String(job.asset_id || '') === assetId || String(job.variant_id || '') === assetId));
}

/** Return whether this image resource is generating from either its asset or durable task state. */
export function dramaAssetImageIsGenerating(asset: ImageResource | undefined, tasks: GenerationTask[] = []) {
  if (!asset) return false;
  return asset.status === '生成中' || tasks.some(task => task.status === '生成中' && taskTargetsImage(task, asset.id));
}

/** Render the shared thumbnail state used anywhere an in-progress asset is referenced. */
export function dramaImageLoadingMarkup(label: string, escapeHtml: (value: unknown) => string) {
  return `<span class="drama-image-loading" role="status" aria-label="${escapeHtml(`${label}图片生成中`)}"><span class="generation-spinner" aria-hidden="true"></span><span class="drama-image-loading-label">生成中</span></span>`;
}
