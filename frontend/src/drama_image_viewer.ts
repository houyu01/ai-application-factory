/** Shared image lightbox for generated drama assets and placeholder images. */

const clampZoom = (value: number) => Math.min(3, Math.max(0.25, value));

function openImageViewer(url: string, label: string) {
  let zoom = 1;
  const modal = document.createElement('div');
  modal.className = 'drama-image-viewer-backdrop';
  modal.innerHTML = `<div class="drama-image-viewer" role="dialog" aria-modal="true" aria-label="图片查看器"><header><div><strong data-image-viewer-title></strong><span data-image-zoom-label>100%</span></div><div class="drama-image-viewer-actions"><button type="button" data-image-zoom-out aria-label="缩小">−</button><button type="button" data-image-zoom-reset>↻ 重置</button><button type="button" data-image-zoom-in aria-label="放大">＋</button><button type="button" class="drama-image-viewer-close" data-image-viewer-close aria-label="关闭">×</button></div></header><main data-image-viewer-stage><img data-image-viewer-image /></main></div>`;
  document.body.append(modal);
  const image = modal.querySelector<HTMLImageElement>('[data-image-viewer-image]')!;
  const title = modal.querySelector<HTMLElement>('[data-image-viewer-title]')!;
  title.textContent = label;
  image.src = url;
  image.alt = label;
  const zoomLabel = modal.querySelector<HTMLElement>('[data-image-zoom-label]')!;
  const updateZoom = (value: number) => { zoom = clampZoom(value); image.style.transform = `scale(${zoom})`; zoomLabel.textContent = `${Math.round(zoom * 100)}%`; };
  const close = () => modal.remove();
  modal.querySelector('[data-image-zoom-out]')?.addEventListener('click', () => updateZoom(zoom - 0.25));
  modal.querySelector('[data-image-zoom-in]')?.addEventListener('click', () => updateZoom(zoom + 0.25));
  modal.querySelector('[data-image-zoom-reset]')?.addEventListener('click', () => updateZoom(1));
  modal.querySelector('[data-image-viewer-close]')?.addEventListener('click', close);
  modal.querySelector('[data-image-viewer-stage]')?.addEventListener('wheel', event => { const wheelEvent = event as WheelEvent; wheelEvent.preventDefault(); updateZoom(zoom + (wheelEvent.deltaY < 0 ? 0.15 : -0.15)); }, { passive: false });
  modal.addEventListener('click', event => { if (event.target === modal) close(); });
  const onKeyDown = (event: KeyboardEvent) => { if (!modal.isConnected) { document.removeEventListener('keydown', onKeyDown); return; } if (event.key === 'Escape') close(); if (event.key === '+' || event.key === '=') updateZoom(zoom + 0.25); if (event.key === '-') updateZoom(zoom - 0.25); if (event.key === '0') updateZoom(1); };
  document.addEventListener('keydown', onKeyDown);
}

document.addEventListener('click', event => {
  const target = event.target instanceof HTMLElement ? event.target : null;
  const trigger = target?.closest<HTMLElement>('[data-drama-image-preview]');
  const url = trigger?.dataset.dramaImagePreview;
  if (!trigger || !url) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  openImageViewer(url, trigger.dataset.dramaImageLabel || '图片详情');
}, true);
