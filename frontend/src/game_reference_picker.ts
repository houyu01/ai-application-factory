/** Interactive-game reference picker and horizontal selected-material list. */

import type { DramaPromptAssetType, DramaPromptNode, Game, GameAsset } from './models.js';

type Runtime = { escapeHtml: (value: unknown) => string; resolveMediaUrl: (value?: string | null) => string };
const kinds: DramaPromptAssetType[] = ['character', 'scene', 'prop', 'placeholder'];
const labels: Record<DramaPromptAssetType, string> = { character: '角色', scene: '场景', prop: '道具', placeholder: '占位图' };

function assets(game: Game, kind?: DramaPromptAssetType) {
  return (game.assets || []).filter(asset => kinds.includes(asset.type as DramaPromptAssetType) && (!kind || asset.type === kind));
}

function reference(asset: GameAsset): Extract<DramaPromptNode, { type: 'reference' }> {
  return { type: 'reference', asset_id: asset.id, asset_type: asset.type as DramaPromptAssetType, label: asset.name, image_url: asset.image_url || null };
}

/** Renders the node's selected reusable materials as a horizontally scrolling card list. */
export function gameReferencePanelMarkup(game: Game, ids: readonly string[], runtime: Runtime) {
  const selected = ids.flatMap(id => { const asset = assets(game).find(item => item.id === id); return asset ? [asset] : []; });
  const cards = selected.length ? selected.map(asset => {
    const image = asset.image_url ? `<img src="${runtime.escapeHtml(runtime.resolveMediaUrl(asset.image_url))}" alt="" />` : '<span>暂无图片</span>';
    return `<article class="game-reference-card" data-game-reference-card="${runtime.escapeHtml(asset.id)}"><span class="game-reference-card-image">${image}</span><span class="game-reference-card-copy"><b>${runtime.escapeHtml(asset.name)}</b><small>${labels[asset.type as DramaPromptAssetType]}</small></span><button type="button" class="game-reference-remove" data-game-remove-reference="${runtime.escapeHtml(asset.id)}" aria-label="移除${runtime.escapeHtml(asset.name)}">×</button></article>`;
  }).join('') : '<p class="game-reference-empty">尚未添加参考图；可从角色、场景、道具或占位图中选择。</p>';
  return `<section class="game-reference-panel"><div class="game-reference-panel-head"><button type="button" class="ghost compact" data-game-add-reference>＋ 添加参考图</button><div><h3>参考图</h3><p>选择当前视频节点使用的素材。</p></div></div><div class="game-reference-scroll">${cards}</div></section>`;
}

/** Opens the short-drama-style asset chooser for multi-select or an @ mention insertion. */
export function openGameReferencePicker(game: Game, selectedIds: readonly string[], runtime: Runtime, onComplete: (ids: string[]) => void, single = false) {
  let activeKind: DramaPromptAssetType = 'character';
  const selected = new Set(selectedIds);
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop drama-reference-picker-backdrop game-reference-picker-backdrop';
  modal.innerHTML = `<div class="modal drama-reference-picker"><button class="close" aria-label="关闭">×</button><div class="modal-head"><h2>${single ? '插入参考图' : '添加参考图'}</h2><p>${single ? '选择一张角色、场景、道具或占位图插入光标位置。' : '选择角色、场景、道具或占位图加入当前视频节点。'}</p></div><div class="drama-reference-picker-tabs">${kinds.map(kind => `<button type="button" class="${kind === activeKind ? 'active' : ''}" data-game-reference-kind="${kind}">${labels[kind]}</button>`).join('')}</div><div class="drama-reference-picker-body"></div><div class="drama-reference-picker-actions"><span class="drama-reference-picker-count">已选择 0 项</span><button type="button" class="ghost" data-game-reference-cancel>取消</button><button type="button" class="primary" data-game-reference-complete ${single ? 'hidden' : ''}>完成</button></div></div>`;
  document.body.append(modal);
  const body = modal.querySelector<HTMLElement>('.drama-reference-picker-body')!;
  const close = () => modal.remove();
  const render = () => {
    const options = assets(game, activeKind);
    body.innerHTML = options.length ? `<div class="drama-reference-picker-grid">${options.map(asset => {
      const checked = selected.has(asset.id); const image = asset.image_url ? `<img src="${runtime.escapeHtml(runtime.resolveMediaUrl(asset.image_url))}" alt="" />` : '<span class="drama-reference-picker-placeholder">＋</span>';
      return `<button type="button" class="drama-reference-option ${checked ? 'selected' : ''}" data-game-reference-option="${runtime.escapeHtml(asset.id)}" aria-pressed="${checked}"><span class="drama-reference-option-image">${image}</span><span class="drama-reference-option-info"><b>${runtime.escapeHtml(asset.name)}</b><small>${asset.image_url ? '已就绪' : '缺少图片'}</small></span><span class="drama-reference-option-check">${checked ? '✓' : ''}</span></button>`;
    }).join('')}</div>` : `<div class="drama-reference-picker-empty"><div>♧</div><p>暂无${labels[activeKind]}素材。</p><small>可先在左侧素材栏配置素材。</small></div>`;
    body.querySelectorAll<HTMLElement>('[data-game-reference-option]').forEach(button => button.addEventListener('click', () => {
      const id = button.dataset.gameReferenceOption || '';
      if (!id) return;
      if (single) { close(); onComplete([id]); return; }
      if (selected.has(id)) selected.delete(id); else selected.add(id);
      render();
    }));
    const count = modal.querySelector<HTMLElement>('.drama-reference-picker-count');
    if (count) count.textContent = `已选择 ${selected.size} 项`;
  };
  modal.querySelectorAll<HTMLElement>('.close,[data-game-reference-cancel]').forEach(button => button.addEventListener('click', close));
  modal.querySelectorAll<HTMLElement>('[data-game-reference-kind]').forEach(button => button.addEventListener('click', () => { activeKind = button.dataset.gameReferenceKind as DramaPromptAssetType; modal.querySelectorAll('[data-game-reference-kind]').forEach(item => item.classList.toggle('active', item === button)); render(); }));
  modal.querySelector('[data-game-reference-complete]')?.addEventListener('click', () => { close(); onComplete(assets(game).filter(asset => selected.has(asset.id)).map(asset => asset.id)); });
  modal.addEventListener('click', event => { if (event.target === modal) close(); });
  render();
}

/** Converts a selected game asset ID into the reference node inserted by the @ editor. */
export function gameReferenceNode(game: Game, assetId: string) {
  const asset = assets(game).find(item => item.id === assetId);
  return asset ? reference(asset) : null;
}
