/** Durable interactive-game cover dialog aligned with the short-drama cover workflow. */
import './drama_cover.css';

import type { Game, GameAsset, GameTask } from './models.js';
import { icon } from './ui_icons.js';

type GameCoverRuntime = {
  apiBaseUrl: string;
  escapeHtml: (value: unknown) => string;
  resolveMediaUrl: (value?: string | null) => string;
  toast: (message: string) => void;
};
type CoverState = { game: Game; characterIds: Set<string>; sceneIds: Set<string>; uploadIds: Set<string>; runtime: GameCoverRuntime; refresh: () => Promise<void> };

let openState: CoverState | null = null;

const assets = (game: Game, type?: string) => (game.assets || []).filter(asset => !type || asset.type === type);
const latestCover = (game: Game) => assets(game, 'cover').sort((a, b) => String(b.created_at || '').localeCompare(String(a.created_at || '')))[0];
const coverTask = (game: Game, cover?: GameAsset) => (game.tasks || []).filter(task => task.type === 'game_cover_image' && task.resource_id === cover?.id).sort((a, b) => String(b.created_at || '').localeCompare(String(a.created_at || '')))[0];
const checkedIds = (cover: GameAsset | undefined, key: 'character_asset_ids' | 'scene_asset_ids' | 'extra_reference_asset_ids') => new Set(Array.isArray(cover?.metadata?.[key]) ? cover!.metadata![key]!.map(String) : []);
const defaultRatio = (game: Game) => game.platform === 'Steam游戏' ? '16:9' : '9:16';
const escape = (value: unknown) => openState!.runtime.escapeHtml(value);
const imageUrl = (asset?: GameAsset) => openState!.runtime.resolveMediaUrl(asset?.image_url);
const assetGenerating = (asset: GameAsset) => asset.status === '生成中' || (openState?.game.tasks || []).some(task => task.status === '生成中' && task.resource_id === asset.id && task.type === 'game_asset_image');

function optionMarkup(asset: GameAsset, selected: boolean) {
  const url = imageUrl(asset);
  const generating = assetGenerating(asset);
  return `<button type="button" class="drama-cover-option${selected ? ' selected' : ''}" data-game-cover-option="${escape(asset.id)}"><span class="drama-cover-option-image">${generating ? '<span class="generation-spinner"></span>' : url ? `<img src="${escape(url)}" alt="" />` : '<span>◇</span>'}</span><span><span class="drama-cover-option-name">${escape(asset.name)}</span><small>${generating ? '图片生成中' : asset.image_url ? '图片已就绪' : '图片尚未生成'}</small></span><span class="drama-cover-check">✓</span></button>`;
}

function selectedCards(type: 'character' | 'scene' | 'cover_reference') {
  const state = openState!;
  const selected = type === 'character' ? state.characterIds : type === 'scene' ? state.sceneIds : state.uploadIds;
  const items = assets(state.game, type).filter(asset => selected.has(asset.id));
  if (!items.length) return '<div class="drama-cover-empty-reference">暂无参考素材</div>';
  return items.map(asset => {
    const url = imageUrl(asset);
    const generating = assetGenerating(asset);
    return `<article class="drama-cover-reference-card"><span>${generating ? '<span class="generation-spinner"></span>' : url ? `<img src="${escape(url)}" alt="" data-drama-image-preview="${escape(url)}" data-drama-image-label="${escape(asset.name)}" />` : '◇'}</span><div><p>${escape(asset.name)}</p><small>${generating ? '图片生成中' : asset.image_url ? '图片已就绪' : '图片尚未生成'}</small></div><button type="button" data-game-cover-remove="${type}:${escape(asset.id)}" aria-label="移除">×</button></article>`;
  }).join('');
}

function previewMarkup(game: Game, cover?: GameAsset) {
  const history = cover?.image_history || [];
  const count = Number(cover?.metadata?.count || 4);
  const ratio = String(cover?.metadata?.ratio || defaultRatio(game));
  if (!history.length) return `<div class="drama-cover-preview-empty"><span>${icon('image')}</span><p>点击生成后会按数量显示封面</p><small>将生成 ${count} 张，比例为 ${escape(ratio)}</small></div>`;
  return `<div class="drama-cover-preview-grid">${history.map((item, index) => {
    const url = openState!.runtime.resolveMediaUrl(item.url);
    return `<button type="button" class="drama-cover-preview-item" data-drama-image-preview="${escape(url)}" data-drama-image-label="${escape(cover?.name || '封面')} ${index + 1}"><img src="${escape(url)}" alt="封面 ${index + 1}" /><span>${index + 1}</span></button>`;
  }).join('')}</div>`;
}

function generateLabel(generating: boolean, failed: boolean) { return generating ? '<span class="generation-spinner"></span><span>生成中...</span>' : failed ? '↻ 重试生成封面' : '生成封面'; }

function referenceSection(type: 'character' | 'scene', title: string, description: string, button: string) {
  return `<section class="drama-cover-reference-section"><div><h3>${title}</h3><p>${description}</p><button type="button" class="ghost compact" data-game-cover-picker="${type}">＋ ${button}</button></div><div class="drama-cover-reference-list" data-game-cover-list="${type}">${selectedCards(type)}</div></section>`;
}

function uploadSection() {
  return `<section class="drama-cover-reference-section"><div><h3>额外参考图</h3><p>上传游戏外的构图、角色服装或风格参考。</p><label class="ghost compact drama-cover-upload">＋ 上传参考图<input type="file" data-game-cover-upload accept="image/*" multiple /></label></div><div class="drama-cover-reference-list" data-game-cover-list="cover_reference">${selectedCards('cover_reference')}</div></section>`;
}

function modalMarkup() {
  const state = openState!;
  const cover = latestCover(state.game);
  const task = coverTask(state.game, cover);
  const generating = task?.status === '生成中' || cover?.status === '生成中';
  const failure = task?.status === '生成失败' ? task.error_message : cover?.status === '生成失败' ? '封面生成失败，请检查图像模型配置。' : '';
  const ratio = String(cover?.metadata?.ratio || defaultRatio(state.game));
  const count = Number(cover?.metadata?.count || 4);
  return `<div class="modal-backdrop drama-cover-backdrop game-cover-backdrop" data-game-cover-backdrop><section class="modal drama-cover-modal game-cover-modal" role="dialog" aria-modal="true" aria-label="封面生成"><header class="drama-cover-head"><div><h2>封面生成</h2><p>${escape(state.game.name)}</p></div><button type="button" class="close" data-game-cover-close aria-label="关闭">×</button></header><div class="drama-cover-body"><div class="drama-cover-form"><label>名称<input id="game-cover-name" value="${escape(cover?.name || state.game.name)}" maxlength="200" /></label><label>封面提示词<textarea id="game-cover-prompt" rows="4" placeholder="例如：突出关键抉择、主角对峙和悬疑氛围。留空会使用默认海报提示词。">${escape(cover?.prompt || '')}</textarea></label><p class="drama-cover-hint">系统会拼接游戏风格、故事背景和选择的角色/场景；这里填写的内容会作为补充要求。</p><div class="drama-cover-inline"><label>生成图的比例<select id="game-cover-ratio">${['9:16', '16:9', '1:1', '3:4', '4:3'].map(value => `<option${value === ratio ? ' selected' : ''}>${value}</option>`).join('')}</select></label><label>生成封面图的数量<select id="game-cover-count">${Array.from({ length: 8 }, (_, index) => index + 1).map(value => `<option value="${value}"${value === count ? ' selected' : ''}>${value} 张</option>`).join('')}</select></label></div>${referenceSection('character', '角色参考', '选择有主图的角色作为封面人物参考。', '添加角色')}${referenceSection('scene', '场景参考', '选择已有场景作为封面环境参考。', '添加场景')}${uploadSection()}</div><aside class="drama-cover-preview"><h3>生成预览</h3><p>选择素材、比例和数量后提交生成。</p><div class="drama-cover-error" data-game-cover-error${failure ? '' : ' hidden'}>${escape(failure || '')}</div><div data-game-cover-preview>${previewMarkup(state.game, cover)}</div></aside></div><footer class="drama-cover-actions"><button type="button" class="ghost" data-game-cover-close>取消</button><button type="button" class="primary${generating ? ' is-loading' : ''}" data-game-cover-generate${generating ? ' disabled' : ''}>${generateLabel(generating, Boolean(failure))}</button></footer></section></div>`;
}

/** Open the game cover workflow with the same reference and batch controls as short-drama cover generation. */
export function openGameCoverModal(game: Game, runtime: GameCoverRuntime, refresh: () => Promise<void>) {
  document.querySelector('[data-game-cover-backdrop]')?.remove();
  const cover = latestCover(game);
  openState = { game, runtime, refresh, characterIds: checkedIds(cover, 'character_asset_ids'), sceneIds: checkedIds(cover, 'scene_asset_ids'), uploadIds: checkedIds(cover, 'extra_reference_asset_ids') };
  document.body.insertAdjacentHTML('beforeend', modalMarkup());
  bindModal();
}

function bindModal() {
  const modal = document.querySelector<HTMLElement>('[data-game-cover-backdrop]');
  if (!modal || !openState) return;
  modal.querySelectorAll<HTMLElement>('[data-game-cover-close]').forEach(button => button.addEventListener('click', closeModal));
  modal.querySelectorAll<HTMLElement>('[data-game-cover-picker]').forEach(button => button.addEventListener('click', () => openPicker(button.dataset.gameCoverPicker as 'character' | 'scene')));
  modal.querySelector<HTMLInputElement>('[data-game-cover-upload]')?.addEventListener('change', event => void uploadReferences((event.target as HTMLInputElement).files));
  modal.querySelector('[data-game-cover-generate]')?.addEventListener('click', () => void generateCover());
  modal.querySelectorAll<HTMLSelectElement>('#game-cover-ratio,#game-cover-count').forEach(select => select.addEventListener('change', refreshEmptyPreviewPlan));
  bindReferenceRemoval(modal);
}

function closeModal() { document.querySelector('[data-game-cover-backdrop]')?.remove(); openState = null; }

function bindReferenceRemoval(root: ParentNode) {
  root.querySelectorAll<HTMLElement>('[data-game-cover-remove]').forEach(button => button.addEventListener('click', () => {
    if (!openState) return;
    const [type, id] = String(button.dataset.gameCoverRemove || '').split(':');
    (type === 'character' ? openState.characterIds : type === 'scene' ? openState.sceneIds : openState.uploadIds).delete(id);
    refreshReferenceLists();
  }));
}

function refreshReferenceLists() {
  if (!openState) return;
  (['character', 'scene', 'cover_reference'] as const).forEach(type => {
    const list = document.querySelector<HTMLElement>(`[data-game-cover-list="${type}"]`);
    if (list) { list.innerHTML = selectedCards(type); bindReferenceRemoval(list); }
  });
}

function refreshEmptyPreviewPlan() {
  if (!openState || latestCover(openState.game)?.image_history?.length) return;
  const preview = document.querySelector<HTMLElement>('[data-game-cover-preview]');
  if (!preview) return;
  const count = Number(document.querySelector<HTMLSelectElement>('#game-cover-count')?.value || 1);
  const ratio = document.querySelector<HTMLSelectElement>('#game-cover-ratio')?.value || defaultRatio(openState.game);
  preview.innerHTML = `<div class="drama-cover-preview-empty"><span>${icon('image')}</span><p>点击生成后会按数量显示封面</p><small>将生成 ${count} 张，比例为 ${escape(ratio)}</small></div>`;
}

function openPicker(type: 'character' | 'scene') {
  if (!openState) return;
  const selected = type === 'character' ? openState.characterIds : openState.sceneIds;
  const picker = document.createElement('div');
  picker.className = 'modal-backdrop drama-cover-picker-backdrop game-cover-picker-backdrop';
  const choices = assets(openState.game, type);
  picker.innerHTML = `<section class="modal drama-cover-picker"><header><div><h2>添加${type === 'character' ? '角色' : '场景'}</h2><p>可先选入封面配置；真正生成封面前，素材图片必须已生成或上传。</p></div><button type="button" class="close" data-game-cover-picker-close>×</button></header><div class="drama-cover-picker-grid">${choices.length ? choices.map(asset => optionMarkup(asset, selected.has(asset.id))).join('') : '<div class="drama-cover-picker-empty">暂无可选素材</div>'}</div><footer><span>已选择 ${selected.size} 项</span><button type="button" class="primary" data-game-cover-picker-done>完成</button></footer></section>`;
  document.body.append(picker);
  const draft = new Set(selected);
  picker.querySelectorAll<HTMLElement>('[data-game-cover-option]').forEach(button => button.addEventListener('click', () => {
    const id = button.dataset.gameCoverOption || '';
    if (draft.has(id)) draft.delete(id); else draft.add(id);
    button.classList.toggle('selected', draft.has(id));
    const label = picker.querySelector('footer span'); if (label) label.textContent = `已选择 ${draft.size} 项`;
  }));
  picker.querySelector('[data-game-cover-picker-done]')?.addEventListener('click', () => { selected.clear(); draft.forEach(id => selected.add(id)); picker.remove(); refreshReferenceLists(); });
  picker.querySelector('[data-game-cover-picker-close]')?.addEventListener('click', () => picker.remove());
}

async function uploadReferences(files: FileList | null) {
  if (!openState || !files?.length) return;
  const state = openState;
  for (const file of Array.from(files)) {
    try {
      const asset = await postJson<GameAsset>(`/games/${state.game.id}/cover-references`, { name: file.name.replace(/\.[^.]+$/, '') || '封面参考图', data_url: await readFile(file) });
      state.game.assets = [...(state.game.assets || []), asset];
      state.uploadIds.add(asset.id);
    } catch (error) { state.runtime.toast(`参考图上传失败：${errorMessage(error)}`); }
  }
  refreshReferenceLists();
  await state.refresh();
}

function readFile(file: File) { return new Promise<string>((resolve, reject) => { const reader = new FileReader(); reader.onload = () => resolve(String(reader.result || '')); reader.onerror = () => reject(reader.error); reader.readAsDataURL(file); }); }

async function generateCover() {
  if (!openState) return;
  const state = openState;
  const button = document.querySelector<HTMLButtonElement>('[data-game-cover-generate]');
  const name = (document.querySelector<HTMLInputElement>('#game-cover-name')?.value || '').trim();
  if (!name) { state.runtime.toast('请填写封面名称'); return; }
  if (button) { button.disabled = true; button.classList.add('is-loading'); button.innerHTML = '<span class="generation-spinner"></span><span>生成中...</span>'; }
  try {
    const payload = await postJson<{ cover: GameAsset; task: GameTask }>(`/games/${state.game.id}/covers/generate`, { name, prompt: document.querySelector<HTMLTextAreaElement>('#game-cover-prompt')?.value || '', ratio: document.querySelector<HTMLSelectElement>('#game-cover-ratio')?.value || defaultRatio(state.game), count: Number(document.querySelector<HTMLSelectElement>('#game-cover-count')?.value || 1), character_asset_ids: [...state.characterIds], scene_asset_ids: [...state.sceneIds], extra_reference_asset_ids: [...state.uploadIds] });
    state.game.assets = [...(state.game.assets || []), payload.cover];
    state.game.tasks = [...(state.game.tasks || []), payload.task];
    state.runtime.toast('封面生成任务已创建');
    await state.refresh();
  } catch (error) {
    if (button) { button.disabled = false; button.classList.remove('is-loading'); button.textContent = '生成封面'; }
    const message = errorMessage(error);
    const errorBox = document.querySelector<HTMLElement>('[data-game-cover-error]');
    if (errorBox) { errorBox.hidden = false; errorBox.textContent = message; }
    state.runtime.toast(`封面生成失败：${message}`);
  }
}

async function postJson<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(`${openState!.runtime.apiBaseUrl}${path}`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
  const payload = await response.json().catch(() => ({})) as T & { detail?: string };
  if (!response.ok) throw new Error(payload.detail || `HTTP ${response.status}`);
  return payload;
}

function errorMessage(error: unknown) { return error instanceof Error ? error.message : '请检查图片模型与参考素材'; }

/** Update the detached cover dialog after game task polling refreshes the durable cover asset. */
export function syncGameCoverUi(game: Game) {
  if (!openState || openState.game.id !== game.id) return;
  openState.game = game;
  refreshReferenceLists();
  const cover = latestCover(game);
  const task = coverTask(game, cover);
  const preview = document.querySelector<HTMLElement>('[data-game-cover-preview]');
  if (preview) preview.innerHTML = previewMarkup(game, cover);
  const failure = task?.status === '生成失败' ? task.error_message : cover?.status === '生成失败' ? '封面生成失败，请检查图像模型配置。' : '';
  const error = document.querySelector<HTMLElement>('[data-game-cover-error]');
  if (error) { error.hidden = !failure; error.textContent = failure || ''; }
  const button = document.querySelector<HTMLButtonElement>('[data-game-cover-generate]');
  if (button) { const generating = task?.status === '生成中' || cover?.status === '生成中'; button.disabled = generating; button.classList.toggle('is-loading', generating); button.innerHTML = generateLabel(generating, Boolean(failure)); }
}
