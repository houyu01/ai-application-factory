/** Display one asset's retained image versions in a focused preview dialog. */
import type { DramaAsset, DramaAssetVariant } from './models.js';

export function renderDramaImageHistoryModal(
  asset: DramaAsset | DramaAssetVariant,
  label: string,
  resolveMediaUrl: (value?: string | null) => string,
  escapeHtml: (value: unknown) => string,
  formatTime: (value?: string) => string,
) {
  const history = asset.image_history || [];
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop';
  modal.innerHTML = `<div class="modal drama-image-history-modal"><button class="close" aria-label="关闭">×</button><div class="modal-head"><h2>${escapeHtml(label)} · 图片历史</h2><p>每次生成都会保留历史版本，可以下载或查看。</p></div><div class="drama-image-history-grid">${history.map((item, index) => { const url = resolveMediaUrl(item.url); return `<a class="drama-image-history-item" href="${escapeHtml(url)}" target="_blank" rel="noopener"><div>${url ? `<img src="${escapeHtml(url)}" alt="版本 ${index + 1}" />` : '暂无图片'}</div><span>版本 ${index + 1}</span><small>${escapeHtml(formatTime(item.generated_at))}</small></a>`; }).join('') || '<p class="muted">暂无图片历史</p>'}</div></div>`;
  document.body.append(modal);
  modal.querySelector('.close')?.addEventListener('click', () => modal.remove());
  modal.addEventListener('click', event => { if (event.target === modal) modal.remove(); });
}
