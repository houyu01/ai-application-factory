/** Durable short-drama cover dialog, reference selection, upload, and previews. */
import './drama_cover.css';

import type { ApiProject, DramaAsset, GenerationTask } from './models.js';
import { dramaAssetImageIsGenerating, dramaImageLoadingMarkup } from './drama_asset_image_state_ui.js';
import { activeDramaProject, dramaViewState, setActiveDramaProject } from './drama_state.js';
import { scheduleDramaTaskRefresh } from './drama_task_polling.js';
import { icon } from './ui_icons.js';

type CoverRuntime = {
  apiBaseUrl: string;
  escapeHtml: (value: unknown) => string;
  toast: (message: string) => void;
  loadDramaDetail: (id: string) => Promise<void>;
  resolveMediaUrl: (value?: string | null) => string;
};

type CoverFormState = {
  project: ApiProject;
  characterIds: Set<string>;
  sceneIds: Set<string>;
  uploadIds: Set<string>;
};

let runtime: CoverRuntime;
let openState: CoverFormState | null = null;
const rt = () => runtime;

export function configureDramaCoverRuntime(value: CoverRuntime) { runtime = value; }

function assets(project: ApiProject, type?: string) {
  return (project.assets || []).filter(asset => !type || asset.type === type);
}

function latestCover(project: ApiProject) {
  return assets(project, 'cover').sort((a, b) => String(b.created_at || '').localeCompare(String(a.created_at || '')))[0];
}

function coverTask(project: ApiProject, cover?: DramaAsset) {
  if (!cover) return undefined;
  return (project.tasks || []).filter(task => task.type === 'cover_image' && task.resource_id === cover.id)
    .sort((a, b) => String(b.created_at || '').localeCompare(String(a.created_at || '')))[0];
}

function checkedAssetIds(cover: DramaAsset | undefined, key: 'character_asset_ids' | 'scene_asset_ids' | 'extra_reference_asset_ids') {
  const value = cover?.metadata?.[key];
  return new Set(Array.isArray(value) ? value.map(String) : []);
}

function optionMarkup(asset: DramaAsset, selected: boolean) {
  const imageUrl = rt().resolveMediaUrl(asset.image_url);
  const generating = dramaAssetImageIsGenerating(asset, openState?.project.tasks);
  return `<button type="button" class="drama-cover-option${selected ? ' selected' : ''}" data-cover-option="${rt().escapeHtml(asset.id)}">
    <span class="drama-cover-option-image">${generating ? dramaImageLoadingMarkup(asset.name, rt().escapeHtml) : imageUrl ? `<img src="${rt().escapeHtml(imageUrl)}" alt="" />` : '<span>◇</span>'}</span>
    <span><span class="drama-cover-option-name">${rt().escapeHtml(asset.name)}</span><small>${generating ? '图片生成中' : asset.image_url ? '图片已就绪' : '图片尚未生成'}</small></span>
    <span class="drama-cover-check">✓</span>
  </button>`;
}

function selectedCards(state: CoverFormState, type: 'character' | 'scene' | 'cover_reference') {
  const selected = type === 'character' ? state.characterIds : type === 'scene' ? state.sceneIds : state.uploadIds;
  const selectedAssets = assets(state.project, type).filter(asset => selected.has(asset.id));
  if (!selectedAssets.length) return '<div class="drama-cover-empty-reference">暂无参考素材</div>';
  return selectedAssets.map(asset => {
    const imageUrl = rt().resolveMediaUrl(asset.image_url);
    const generating = dramaAssetImageIsGenerating(asset, state.project.tasks);
    return `<article class="drama-cover-reference-card">
      <span>${generating ? dramaImageLoadingMarkup(asset.name, rt().escapeHtml) : imageUrl ? `<img src="${rt().escapeHtml(imageUrl)}" alt="" data-drama-image-preview="${rt().escapeHtml(imageUrl)}" data-drama-image-label="${rt().escapeHtml(asset.name)}" />` : '◇'}</span>
      <div><p>${rt().escapeHtml(asset.name)}</p><small>${generating ? '图片生成中' : asset.image_url ? '图片已就绪' : '图片尚未生成'}</small></div>
      <button type="button" data-cover-remove="${rt().escapeHtml(type)}:${rt().escapeHtml(asset.id)}" aria-label="移除">×</button>
    </article>`;
  }).join('');
}

function previewMarkup(project: ApiProject, cover?: DramaAsset) {
  const history = cover?.image_history || [];
  const count = Number(cover?.metadata?.count || 1);
  const ratio = String(cover?.metadata?.ratio || project.ratio || '9:16');
  if (!history.length) return `<div class="drama-cover-preview-empty"><span>${icon('image')}</span><p>点击生成后会按数量显示封面</p><small>将生成 ${count} 张，比例为 ${rt().escapeHtml(ratio)}</small></div>`;
  return `<div class="drama-cover-preview-grid">${history.map((item, index) => {
    const url = rt().resolveMediaUrl(item.url);
    return `<button type="button" class="drama-cover-preview-item" data-drama-image-preview="${rt().escapeHtml(url)}" data-drama-image-label="${rt().escapeHtml(cover?.name || '封面')} ${index + 1}"><img src="${rt().escapeHtml(url)}" alt="封面 ${index + 1}" /><span>${index + 1}</span></button>`;
  }).join('')}</div>`;
}

function modalMarkup(state: CoverFormState) {
  const project = state.project;
  const cover = latestCover(project);
  const task = coverTask(project, cover);
  const generating = task?.status === '生成中' || cover?.status === '生成中';
  const failed = task?.status === '生成失败' ? task.error_message : cover?.status === '生成失败' ? '封面生成失败，请检查图像模型配置。' : '';
  const ratio = String(cover?.metadata?.ratio || project.ratio || '9:16');
  const count = Number(cover?.metadata?.count || 4);
  return `<div class="modal-backdrop drama-cover-backdrop" data-cover-backdrop>
    <section class="modal drama-cover-modal" role="dialog" aria-modal="true" aria-label="封面生成">
      <header class="drama-cover-head"><div><h2>封面生成</h2><p>${rt().escapeHtml(project.name)}</p></div><button type="button" class="close" data-cover-close aria-label="关闭">×</button></header>
      <div class="drama-cover-body">
        <div class="drama-cover-form">
          <label>名称<input id="cover-name" value="${rt().escapeHtml(cover?.name || project.name)}" maxlength="200" /></label>
          <label>封面提示词<textarea id="cover-prompt" rows="4" placeholder="例如：突出双人对峙、暖色调、悬疑氛围。留空会使用默认海报提示词。">${rt().escapeHtml(cover?.prompt || '')}</textarea></label>
          <p class="drama-cover-hint">系统会拼接项目风格、主题和选择的角色/场景；这里填写的内容会作为补充要求。</p>
          <div class="drama-cover-inline">
            <label>生成图的比例<select id="cover-ratio">${['9:16', '16:9', '1:1', '3:4', '4:3'].map(value => `<option${value === ratio ? ' selected' : ''}>${value}</option>`).join('')}</select></label>
            <label>生成封面图的数量<select id="cover-count">${Array.from({ length: 8 }, (_, index) => index + 1).map(value => `<option value="${value}"${value === count ? ' selected' : ''}>${value} 张</option>`).join('')}</select></label>
          </div>
          ${referenceSection(state, 'character', '角色参考', '选择有主图的角色作为封面人物参考。', '添加角色')}
          ${referenceSection(state, 'scene', '场景参考', '选择已有场景作为封面环境参考。', '添加场景')}
          ${uploadSection(state)}
        </div>
        <aside class="drama-cover-preview"><h3>生成预览</h3><p>选择素材、比例和数量后提交生成。</p><div class="drama-cover-error" data-cover-error${failed ? '' : ' hidden'}>${rt().escapeHtml(failed)}</div><div data-cover-preview>${previewMarkup(project, cover)}</div></aside>
      </div>
      <footer class="drama-cover-actions"><button type="button" class="ghost" data-cover-close>取消</button><button type="button" class="primary" data-cover-generate${generating ? ' disabled' : ''}>${generating ? '<span class="generation-spinner"></span><span>生成中...</span>' : '生成封面'}</button></footer>
    </section>
  </div>`;
}

function referenceSection(state: CoverFormState, type: 'character' | 'scene', title: string, description: string, button: string) {
  return `<section class="drama-cover-reference-section"><div><h3>${title}</h3><p>${description}</p><button type="button" class="ghost compact" data-cover-picker="${type}">＋ ${button}</button></div><div class="drama-cover-reference-list" data-cover-list="${type}">${selectedCards(state, type)}</div></section>`;
}

function uploadSection(state: CoverFormState) {
  return `<section class="drama-cover-reference-section"><div><h3>额外参考图</h3><p>上传项目外的构图、服装或风格参考。</p><label class="ghost compact drama-cover-upload">＋ 上传参考图<input type="file" data-cover-upload accept="image/*" multiple /></label></div><div class="drama-cover-reference-list" data-cover-list="cover_reference">${selectedCards(state, 'cover_reference')}</div></section>`;
}

export function openDramaCoverModal(project: ApiProject) {
  document.querySelector('[data-cover-backdrop]')?.remove();
  const cover = latestCover(project);
  openState = {
    project,
    characterIds: checkedAssetIds(cover, 'character_asset_ids'),
    sceneIds: checkedAssetIds(cover, 'scene_asset_ids'),
    uploadIds: checkedAssetIds(cover, 'extra_reference_asset_ids'),
  };
  document.body.insertAdjacentHTML('beforeend', modalMarkup(openState));
  bindModal();
}

function bindModal() {
  const modal = document.querySelector<HTMLElement>('[data-cover-backdrop]');
  if (!modal || !openState) return;
  modal.querySelectorAll<HTMLElement>('[data-cover-close]').forEach(button => button.addEventListener('click', closeModal));
  modal.querySelectorAll<HTMLElement>('[data-cover-picker]').forEach(button => button.addEventListener('click', () => openPicker(button.dataset.coverPicker as 'character' | 'scene')));
  modal.querySelector<HTMLInputElement>('[data-cover-upload]')?.addEventListener('change', event => void uploadReferences((event.target as HTMLInputElement).files));
  modal.querySelector('[data-cover-generate]')?.addEventListener('click', () => void generateCover());
  modal.querySelectorAll<HTMLSelectElement>('#cover-ratio,#cover-count').forEach(select => select.addEventListener('change', refreshEmptyPreviewPlan));
  bindReferenceRemoval(modal);
}

function closeModal() { document.querySelector('[data-cover-backdrop]')?.remove(); openState = null; }

function bindReferenceRemoval(root: ParentNode) {
  root.querySelectorAll<HTMLElement>('[data-cover-remove]').forEach(button => button.addEventListener('click', () => {
    if (!openState) return;
    const [type, id] = String(button.dataset.coverRemove || '').split(':');
    (type === 'character' ? openState.characterIds : type === 'scene' ? openState.sceneIds : openState.uploadIds).delete(id);
    refreshReferenceLists();
  }));
}

function refreshReferenceLists() {
  if (!openState) return;
  (['character', 'scene', 'cover_reference'] as const).forEach(type => {
    const list = document.querySelector<HTMLElement>(`[data-cover-list="${type}"]`);
    if (list) {
      list.innerHTML = selectedCards(openState!, type);
      bindReferenceRemoval(list);
    }
  });
}

function refreshEmptyPreviewPlan() {
  if (!openState || latestCover(openState.project)?.image_history?.length) return;
  const preview = document.querySelector<HTMLElement>('[data-cover-preview]');
  if (!preview) return;
  const count = Number(document.querySelector<HTMLSelectElement>('#cover-count')?.value || 1);
  const ratio = document.querySelector<HTMLSelectElement>('#cover-ratio')?.value || openState.project.ratio || '9:16';
  preview.innerHTML = `<div class="drama-cover-preview-empty"><span>${icon('image')}</span><p>点击生成后会按数量显示封面</p><small>将生成 ${count} 张，比例为 ${rt().escapeHtml(ratio)}</small></div>`;
}

function openPicker(type: 'character' | 'scene') {
  if (!openState) return;
  const selected = type === 'character' ? openState.characterIds : openState.sceneIds;
  const choices = assets(openState.project, type);
  const picker = document.createElement('div');
  picker.className = 'modal-backdrop drama-cover-picker-backdrop';
  picker.innerHTML = `<section class="modal drama-cover-picker"><header><div><h2>添加${type === 'character' ? '角色' : '场景'}</h2><p>可先选入封面配置；真正生成封面前，素材图片必须已生成或上传。</p></div><button type="button" class="close" data-picker-close>×</button></header><div class="drama-cover-picker-grid">${choices.length ? choices.map(asset => optionMarkup(asset, selected.has(asset.id))).join('') : '<div class="drama-cover-picker-empty">暂无可选素材</div>'}</div><footer><span>已选择 ${selected.size} 项</span><button type="button" class="primary" data-picker-done>完成</button></footer></section>`;
  document.body.append(picker);
  const draft = new Set(selected);
  picker.querySelectorAll<HTMLElement>('[data-cover-option]').forEach(button => button.addEventListener('click', () => {
    const id = button.dataset.coverOption || '';
    if (draft.has(id)) draft.delete(id); else draft.add(id);
    button.classList.toggle('selected', draft.has(id));
    const label = picker.querySelector('footer span'); if (label) label.textContent = `已选择 ${draft.size} 项`;
  }));
  picker.querySelector('[data-picker-done]')?.addEventListener('click', () => { selected.clear(); draft.forEach(id => selected.add(id)); picker.remove(); refreshReferenceLists(); });
  picker.querySelector('[data-picker-close]')?.addEventListener('click', () => picker.remove());
}

async function uploadReferences(files: FileList | null) {
  if (!openState || !files?.length) return;
  for (const file of Array.from(files)) {
    try {
      const created = await postJson<DramaAsset>(`/projects/${openState.project.id}/assets`, { type: 'cover_reference', name: file.name.replace(/\.[^.]+$/, '') || '封面参考图', prompt: '用户上传的封面参考图' });
      const uploaded = await postJson<DramaAsset>(`/projects/${openState.project.id}/assets/${created.id}/upload`, { data_url: await readFile(file) });
      openState.project.assets = [...(openState.project.assets || []).filter(item => item.id !== uploaded.id), uploaded];
      openState.uploadIds.add(uploaded.id);
    } catch (error) { rt().toast(`参考图上传失败：${errorMessage(error)}`); }
  }
  setActiveDramaProject(openState.project);
  refreshReferenceLists();
}

function readFile(file: File) { return new Promise<string>((resolve, reject) => { const reader = new FileReader(); reader.onload = () => resolve(String(reader.result || '')); reader.onerror = () => reject(reader.error); reader.readAsDataURL(file); }); }

async function generateCover() {
  if (!openState) return;
  const button = document.querySelector<HTMLButtonElement>('[data-cover-generate]');
  const name = (document.querySelector<HTMLInputElement>('#cover-name')?.value || '').trim();
  if (!name) { rt().toast('请填写封面名称'); return; }
  if (button) { button.disabled = true; button.innerHTML = '<span class="generation-spinner"></span><span>生成中...</span>'; }
  try {
    const payload = await postJson<{ cover: DramaAsset; task: GenerationTask }>(`/projects/${openState.project.id}/covers/generate`, {
      name,
      prompt: document.querySelector<HTMLTextAreaElement>('#cover-prompt')?.value || '',
      ratio: document.querySelector<HTMLSelectElement>('#cover-ratio')?.value || openState.project.ratio || '9:16',
      count: Number(document.querySelector<HTMLSelectElement>('#cover-count')?.value || 1),
      character_asset_ids: [...openState.characterIds], scene_asset_ids: [...openState.sceneIds], extra_reference_asset_ids: [...openState.uploadIds],
    });
    openState.project.assets = [...(openState.project.assets || []), payload.cover];
    openState.project.tasks = [...(openState.project.tasks || []), payload.task];
    setActiveDramaProject(openState.project);
    syncDramaCoverUi(openState.project);
    scheduleDramaTaskRefresh(openState.project);
    rt().toast('封面生成任务已创建');
  } catch (error) {
    if (button) { button.disabled = false; button.textContent = '生成封面'; }
    const message = errorMessage(error);
    const errorBox = document.querySelector<HTMLElement>('[data-cover-error]');
    if (errorBox) { errorBox.hidden = false; errorBox.textContent = message; }
    rt().toast(`封面生成失败：${message}`);
  }
}

async function postJson<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(`${rt().apiBaseUrl}${path}`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
  const payload = await response.json().catch(() => ({})) as T & { detail?: string };
  if (!response.ok) throw new Error(payload.detail || `HTTP ${response.status}`);
  return payload;
}

function errorMessage(error: unknown) { return error instanceof Error ? error.message : '请检查图片模型与参考素材'; }

export function syncDramaCoverUi(project: ApiProject) {
  if (!openState || openState.project.id !== project.id) return;
  openState.project = project;
  const preview = document.querySelector<HTMLElement>('[data-cover-preview]');
  const cover = latestCover(project);
  const task = coverTask(project, cover);
  if (preview) preview.innerHTML = previewMarkup(project, cover);
  const error = document.querySelector<HTMLElement>('[data-cover-error]');
  const failure = task?.status === '生成失败' ? task.error_message : cover?.status === '生成失败' ? '封面生成失败，请检查图像模型配置。' : '';
  if (error) { error.hidden = !failure; error.textContent = failure || ''; }
  const button = document.querySelector<HTMLButtonElement>('[data-cover-generate]');
  if (button) { const generating = task?.status === '生成中' || cover?.status === '生成中'; button.disabled = generating; button.innerHTML = generating ? '<span class="generation-spinner"></span><span>生成中...</span>' : '生成封面'; }
}

function ensureCoverRailItem() {
  const rail = document.querySelector<HTMLElement>('.drama-detail .drama-asset-rail');
  if (!rail || rail.querySelector('[data-drama-cover-rail]')) return;
  const button = document.createElement('button');
  button.type = 'button'; button.className = 'drama-asset-rail-item'; button.dataset.dramaCoverRail = 'true'; button.title = '生成短剧封面';
  button.innerHTML = `<span class="drama-asset-rail-icon">${icon('image')}</span><span>封面</span>`;
  button.addEventListener('click', event => { event.preventDefault(); event.stopPropagation(); if (activeDramaProject) openDramaCoverModal(activeDramaProject); else if (dramaViewState.projectId) void rt().loadDramaDetail(dramaViewState.projectId); });
  rail.append(button);
}

const observer = new MutationObserver(ensureCoverRailItem);
observer.observe(document.querySelector('#app')!, { childList: true, subtree: true });
