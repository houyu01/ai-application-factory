/** Opens the shot-reference picker with base assets and independently selectable character forms. */
import { dramaAssetImageIsGenerating, dramaImageLoadingMarkup } from './drama_asset_image_state_ui.js';
import { dramaReferenceKey, dramaReferenceOptions } from './drama_reference_asset.js';
import { selectedDramaReferenceNodes } from './drama_reference_picker_selection.js';
import type { ApiProject, DramaPromptAssetType, DramaPromptNode } from './models.js';

type Runtime = {
  getAssets: (project: ApiProject) => ApiProject['assets'];
  resolveMediaUrl: (url?: string | null) => string;
  escapeHtml: (value: unknown) => string;
};

let runtime: Runtime | null = null;

function label(kind: DramaPromptAssetType) {
  return ({ character: '角色', scene: '场景', prop: '道具', placeholder: '占位图' } as Record<string, string>)[kind];
}

function openPicker(project: ApiProject, existingNodes: DramaPromptNode[], onComplete: (nodes: Extract<DramaPromptNode, { type: 'reference' }>[]) => void, single = false) {
  if (!runtime) return;
  let activeKind: DramaPromptAssetType = 'character';
  const selected = new Set<string>();
  const existingReferences = existingNodes.filter((node): node is Extract<DramaPromptNode, { type: 'reference' }> => node.type === 'reference');
  const existing = new Set(existingReferences.map(dramaReferenceKey));
  existing.forEach(key => selected.add(key));
  const nodeById = new Map<string, Extract<DramaPromptNode, { type: 'reference' }>>(existingReferences.map(node => [dramaReferenceKey(node), node]));
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop drama-reference-picker-backdrop';
  modal.innerHTML = `<div class="modal drama-reference-picker"><button class="close" aria-label="关闭">×</button><div class="modal-head"><h2>${single ? '插入参考图' : '添加参考图'}</h2><p>${single ? '选择一张角色、角色形态、场景或道具图插入光标位置。' : '选择角色、角色形态、场景或道具加入当前分镜。'}</p></div><div class="drama-reference-picker-tabs">${(['character', 'scene', 'prop', 'placeholder'] as DramaPromptAssetType[]).map(kind => `<button type="button" class="${kind === activeKind ? 'active' : ''}" data-reference-kind="${kind}">${label(kind)}</button>`).join('')}</div><div class="drama-reference-picker-body"></div><div class="drama-reference-picker-actions"><span class="drama-reference-picker-count">${single ? '点击素材即可插入' : '已选择 0 项'}</span><button type="button" class="ghost" data-reference-cancel>取消</button><button type="button" class="primary" data-reference-complete ${single ? 'hidden' : ''}>完成</button></div></div>`;
  document.body.append(modal);
  const body = modal.querySelector<HTMLElement>('.drama-reference-picker-body')!;
  const render = () => {
    const options = dramaReferenceOptions(runtime!.getAssets(project) || [], activeKind);
    options.forEach(option => nodeById.set(option.key, option.node));
    body.innerHTML = options.length ? `<div class="drama-reference-picker-grid">${options.map(option => {
      const added = existing.has(option.key); const checked = selected.has(option.key);
      const generating = dramaAssetImageIsGenerating(option.asset, project.tasks); const imageUrl = option.asset.image_url;
      const image = generating ? dramaImageLoadingMarkup(option.asset.name, runtime!.escapeHtml) : imageUrl ? `<img src="${runtime!.escapeHtml(runtime!.resolveMediaUrl(imageUrl))}" alt="" />` : '<span class="drama-reference-picker-placeholder">＋</span>';
      const status = generating ? '生成中' : imageUrl ? '已就绪' : '缺少图片';
      return `<button type="button" class="drama-reference-option ${checked ? 'selected' : ''} ${added ? 'already-added' : ''} ${!imageUrl && !generating ? 'missing' : ''}" data-reference-option="${runtime!.escapeHtml(option.key)}" aria-pressed="${checked}"><span class="drama-reference-option-image">${image}</span><span class="drama-reference-option-info"><b>${runtime!.escapeHtml(option.asset.name)}</b><small>${added ? (checked ? '当前已选，可点击取消' : '已移除') : status}</small></span><span class="drama-reference-option-check">${checked ? '✓' : ''}</span></button>`;
    }).join('')}</div>` : `<div class="drama-reference-picker-empty"><div>♧</div><p>暂无${label(activeKind)}素材。</p><small>未生成图片的素材也可以先添加，生成视频前需要先补齐图片。</small></div>`;
    body.querySelectorAll<HTMLElement>('[data-reference-option]').forEach(button => button.addEventListener('click', () => {
      const key = button.dataset.referenceOption || ''; if (!key) return;
      if (single) { const node = nodeById.get(key); if (node) { close(); onComplete([node]); } return; }
      if (selected.has(key)) selected.delete(key); else selected.add(key); render();
    }));
    const count = modal.querySelector<HTMLElement>('.drama-reference-picker-count'); if (count) count.textContent = `已选择 ${selected.size} 项`;
  };
  const close = () => modal.remove();
  modal.querySelectorAll<HTMLElement>('.close,[data-reference-cancel]').forEach(button => button.addEventListener('click', close));
  modal.querySelectorAll<HTMLElement>('[data-reference-kind]').forEach(button => button.addEventListener('click', () => { activeKind = button.dataset.referenceKind as DramaPromptAssetType; modal.querySelectorAll('[data-reference-kind]').forEach(item => item.classList.toggle('active', item === button)); render(); }));
  modal.querySelector('[data-reference-complete]')?.addEventListener('click', () => { const nodes = selectedDramaReferenceNodes(selected, nodeById); close(); onComplete(nodes); });
  modal.addEventListener('click', event => { if (event.target === modal) close(); });
  render();
}

/** Reuse the form-aware picker from an editor button that needs a custom save callback. */
export function openDramaReferencePicker(project: ApiProject, existingNodes: DramaPromptNode[], onComplete: (nodes: Extract<DramaPromptNode, { type: 'reference' }>[]) => void) {
  openPicker(project, existingNodes, onComplete);
}

/** Opens a one-shot picker used by the rich editor after the user types @. */
export function openDramaReferenceMentionPicker(project: ApiProject, onComplete: (node: Extract<DramaPromptNode, { type: 'reference' }>) => void) {
  openPicker(project, [], nodes => { if (nodes[0]) onComplete(nodes[0]); }, true);
}

/** Configure the asset and media helpers shared by the picker. */
export function configureDramaReferencePicker(value: Runtime) {
  runtime = value;
}
