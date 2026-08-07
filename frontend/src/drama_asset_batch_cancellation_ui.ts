/** Keep an asset drawer's bulk-image cancellation control aligned with durable tasks. */
import type { ApiProject, DramaAssetKind } from './models.js';

type CancellationOptions = {
  apiBaseUrl: string;
  project: ApiProject;
  assetType: DramaAssetKind | null;
  toast: (message: string) => void;
  reloadProject: (projectId: string) => Promise<void>;
};

type CancellationResult = { cancelled_count: number };

let current: CancellationOptions | null = null;

function activeImageTaskCount(project: ApiProject, assetType: DramaAssetKind | null) {
  if (!assetType || !['character', 'scene', 'prop'].includes(assetType)) return 0;
  const assets = (project.assets || []).filter(asset => asset.type === assetType);
  const assetIds = new Set(assets.map(asset => asset.id));
  const variantIds = new Set(assets.flatMap(asset => asset.variants || []).map(variant => variant.id));
  return (project.tasks || []).filter(task => task.status === '生成中' && (
    (task.type === 'asset_image_batch' && task.resource_id === assetType)
    || (task.type === 'asset_image' && assetIds.has(task.resource_id || ''))
    || (task.type === 'asset_variant_image' && variantIds.has(task.resource_id || ''))
  )).length;
}

async function readError(response: Response) {
  const payload = await response.json().catch(() => ({})) as { detail?: unknown };
  return typeof payload.detail === 'string' ? payload.detail : `HTTP ${response.status}`;
}

/** Bind the selected drawer tab's cancellation button without touching other image work. */
export function syncDramaAssetBatchCancellation(options: CancellationOptions) {
  current = options;
  const button = document.querySelector<HTMLButtonElement>('[data-drama-cancel-asset-images]');
  if (!button) return;
  if (button.dataset.cancelling !== 'true') {
    const count = activeImageTaskCount(options.project, options.assetType);
    button.disabled = count === 0;
    button.title = count ? `取消 ${count} 个进行中的${options.assetType === 'character' ? '角色' : options.assetType === 'scene' ? '场景' : '道具'}图片任务` : '当前没有可取消的图片任务';
  }
  if (button.dataset.assetCancellationBound === 'true') return;
  button.dataset.assetCancellationBound = 'true';
  button.addEventListener('click', async () => {
    const request = current;
    if (!request || !request.assetType || button.disabled) return;
    button.dataset.cancelling = 'true';
    button.disabled = true;
    button.textContent = '取消中…';
    try {
      const response = await fetch(
        `${request.apiBaseUrl}/projects/${encodeURIComponent(request.project.id)}/assets/${encodeURIComponent(request.assetType)}/images/cancel`,
        { method: 'POST' },
      );
      if (!response.ok) throw new Error(await readError(response));
      const result = await response.json() as CancellationResult;
      request.toast(`已取消 ${result.cancelled_count} 个${request.assetType === 'character' ? '角色' : request.assetType === 'scene' ? '场景' : '道具'}图片任务`);
      await request.reloadProject(request.project.id);
    } catch (error) {
      button.dataset.cancelling = 'false';
      button.textContent = '取消生成';
      button.disabled = false;
      request.toast(`取消图片生成失败：${error instanceof Error ? error.message : '请稍后重试'}`);
    }
  });
}
