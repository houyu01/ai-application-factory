/** Short-drama-style interactive-game asset workbench with durable image tasks and alternate forms. */

import type { Game, GameAsset, GameAssetImageHistory, GameAssetVariant, GameTask } from './models.js';
import { applyQueuedGameImageTask } from './game_asset_image_task_state.js';
import { icon } from './ui_icons.js';
import { gameAssetKinds, gameMaterialLabel } from './game_material_rail.js';
import { gameAssetPublicPrompt, gameAssetPublicPromptDefault } from './game_asset_public_prompt.js';
import { closeGameAssetDrawer } from './game_asset_drawer_cleanup.js';
import type { GameMaterialRuntime } from './game_materials_ui.js';

type AssetKind = typeof gameAssetKinds[number]['type'];
type AssetItem = GameAsset | GameAssetVariant;

const kinds = gameAssetKinds.map(item => item.type);
const escape = (rt: GameMaterialRuntime, value: unknown) => rt.escapeHtml(value);
const label = (kind: string) => gameMaterialLabel(kind);
const assetsOf = (game: Game, kind: AssetKind) => (game.assets || []).filter(asset => asset.type === kind);
const assetOf = (game: Game, id: string) => (game.assets || []).find(asset => asset.id === id);
const taskFor = (game: Game, type: string, resourceId: string) => [...(game.tasks || [])].reverse().find(task => task.type === type && task.resource_id === resourceId);

async function errorMessage(response: Response) {
  const body = await response.json().catch(() => null) as { detail?: unknown; message?: unknown } | null;
  return typeof body?.detail === 'string' ? body.detail : typeof body?.message === 'string' ? body.message : `HTTP ${response.status}`;
}

function statusClass(status: string) { return status === '生成中' ? 'running' : status === '生成失败' ? 'failed' : ''; }
function sourceUrl(rt: GameMaterialRuntime, item: AssetItem) { return item.image_url ? escape(rt, rt.resolveMediaUrl(item.image_url)) : ''; }
function imageMarkup(item: AssetItem, type: string, rt: GameMaterialRuntime, generating: boolean) {
  if (generating || item.status === '生成中') return `<div class="drama-image-loading"><span class="generation-spinner"></span><small>生成中</small></div>`;
  const url = sourceUrl(rt, item);
  if (url) return `<button type="button" class="drama-image-preview-trigger" data-game-preview-image="${url}" data-game-preview-label="${escape(rt, item.name)}"><img src="${url}" alt="${escape(rt, item.name)}" /></button>`;
  return `<div class="drama-asset-placeholder">${type === 'character' ? icon('character') : type === 'scene' ? '✦' : '◆'}</div>`;
}

function historyButton(item: AssetItem, assetId: string, rt: GameMaterialRuntime) {
  const count = item.image_history?.length || 0;
  return `<button class="ghost compact" data-game-image-history="${escape(rt, item.id)}" data-game-parent-asset="${escape(rt, assetId)}" ${count ? '' : 'disabled'}>${icon('history')}<span>图片历史${count ? `（${count}）` : ''}</span></button>`;
}

function variantCard(game: Game, asset: GameAsset, variant: GameAssetVariant, rt: GameMaterialRuntime) {
  const task = taskFor(game, 'game_asset_variant_image', variant.id);
  return `<article class="drama-asset-variant-card"><div class="drama-asset-variant-image">${imageMarkup(variant, asset.type, rt, task?.status === '生成中')}</div><div class="drama-asset-variant-body"><div class="drama-asset-heading"><div><h4>${escape(rt, variant.name)}</h4><span>${escape(rt, variant.id.slice(-8))}</span></div><span class="status ${statusClass(variant.status)}">${escape(rt, variant.status)}</span></div><p>${escape(rt, variant.prompt || '等待生成形态提示词')}</p><div class="drama-asset-actions"><button class="small-btn" data-game-generate-variant="${escape(rt, asset.id)}" data-game-variant-id="${escape(rt, variant.id)}">${task?.status === '生成中' ? '生成中…' : `${icon('sparkle')}<span>生成图片</span>`}</button><button class="ghost compact" data-game-edit-variant="${escape(rt, asset.id)}" data-game-variant-id="${escape(rt, variant.id)}">${icon('edit')}<span>编辑</span></button>${historyButton(variant, asset.id, rt)}<button class="danger-button compact" data-game-delete-variant="${escape(rt, asset.id)}" data-game-variant-id="${escape(rt, variant.id)}">${icon('trash')}<span>删除</span></button></div></div></article>`;
}

function assetCard(game: Game, asset: GameAsset, rt: GameMaterialRuntime) {
  const imageTask = taskFor(game, 'game_asset_image', asset.id);
  const failure = asset.status === '生成失败' ? imageTask?.error_message : '';
  const variants = asset.variants || [];
  const variantButtons = `<button class="ghost compact" data-game-add-variant="${escape(rt, asset.id)}">${icon('plus')}<span>添加形态</span></button>${asset.type === 'character' ? `<button class="ghost compact" data-game-change-outfit="${escape(rt, asset.id)}">${icon('shirt')}<span>换装</span></button>` : ''}`;
  const variantsMarkup = `<details class="drama-asset-variants" ${variants.length ? '' : 'hidden'}><summary>展开其他形态 <span>${variants.length} 个其他形态</span></summary><div class="drama-asset-variant-list">${variants.map(variant => variantCard(game, asset, variant, rt)).join('')}</div></details>`;
  const download = asset.image_url ? `<a class="drama-icon-button" href="${sourceUrl(rt, asset)}" download target="_blank" rel="noopener" title="下载图片">${icon('download')}</a>` : '';
  return `<article class="drama-asset-card game-material-card" data-game-asset-card="${escape(rt, asset.id)}" data-asset-type="${escape(rt, asset.type)}" data-asset-name="${escape(rt, asset.name.toLowerCase())}"><div class="drama-asset-main"><div class="drama-asset-image">${imageMarkup(asset, asset.type, rt, imageTask?.status === '生成中')}</div><div class="drama-asset-body"><div class="drama-asset-heading"><div><h3>${escape(rt, asset.name)} <small>${escape(rt, asset.id.slice(-8))}</small></h3><span>${escape(rt, label(asset.type))} · 基础形态</span></div><div class="drama-asset-card-tools">${download}<button class="drama-icon-button" data-game-edit-asset="${escape(rt, asset.id)}" title="编辑">${icon('edit')}</button><button class="drama-icon-button danger" data-game-delete-asset="${escape(rt, asset.id)}" title="删除">${icon('trash')}</button></div></div><div class="drama-asset-badges"><span class="status ${statusClass(asset.status)}">${escape(rt, asset.status)}</span><span class="drama-asset-form-badge">基础形态</span></div>${failure ? `<div class="drama-asset-error"><b>生成失败原因：</b>${escape(rt, failure)}</div>` : ''}<p class="drama-asset-alias"><b>别名：</b>${escape(rt, asset.name)} / ${escape(rt, asset.name)}</p><p class="drama-asset-prompt"><b>图片提示词：</b>${escape(rt, asset.prompt || '等待生成素材提示词')}</p>${asset.type === 'character' ? '' : variantsMarkup}</div></div><div class="drama-asset-actions"><button class="small-btn" data-game-generate-asset="${escape(rt, asset.id)}">${imageTask?.status === '生成中' ? '生成中…' : `${icon('sparkle')}<span>生成图片</span>`}</button><button class="ghost compact" data-game-upload-asset="${escape(rt, asset.id)}">${icon('upload')}<span>上传${escape(rt, label(asset.type))}</span></button>${variantButtons}${historyButton(asset, asset.id, rt)}</div>${asset.type === 'character' ? variantsMarkup : ''}</article>`;
}

export function gameAssetDrawerMarkup(game: Game, kind: AssetKind, rt: GameMaterialRuntime, opening = true) {
  const assets = assetsOf(game, kind);
  const completed = assets.filter(asset => ['生成成功', '已配置'].includes(asset.status)).length;
  const state = completed === assets.length && assets.length ? '已完成' : assets.some(asset => asset.status === '生成中') ? '生成中' : '待生成';
  return `<div class="drama-sheet-backdrop game-material-sheet-backdrop" data-game-material-sheet><aside class="drama-asset-sheet ${opening ? 'is-opening ' : ''}game-material-sheet"><div class="drama-sheet-head"><div><div class="eyebrow">素材库 / ${escape(rt, label(kind))}</div><h2>${escape(rt, label(kind))}素材 <span class="sheet-badge">${state}</span></h2><p>共 ${assets.length} 个素材${assets.length ? `，${completed} 个已完成` : ''}</p></div><button class="close sheet-close" data-game-close-sheet aria-label="关闭">×</button></div><div class="drama-sheet-tabs">${kinds.map(item => `<button class="${item === kind ? 'active' : ''}" data-game-material-tab="${item}">${label(item)} <small>${assetsOf(game, item).length}</small></button>`).join('')}</div><div class="drama-sheet-toolbar drama-sheet-toolbar-primary"><button class="primary drama-sheet-button" data-game-generate-all-assets><span class="drama-button-icon">${icon('image')}</span><span>生成全部图片</span></button><button class="ghost drama-sheet-button" data-game-open-asset-public><span class="drama-button-icon">${icon('square')}</span><span>公共提示词</span></button></div><div class="drama-sheet-toolbar drama-sheet-toolbar-secondary"><button class="ghost drama-sheet-button" data-game-add-asset><span class="drama-button-icon">${icon('plus')}</span><span>添加${label(kind)}</span></button><button class="ghost drama-sheet-button" data-game-refresh-assets><span class="drama-button-icon">${icon('refresh')}</span><span>刷新</span></button><span class="drama-sheet-toolbar-spacer"></span><button class="ghost compact drama-sheet-button" data-game-close-sheet><span class="drama-button-icon">${icon('collapse')}</span><span>收起</span></button><button class="ghost compact drama-sheet-button drama-sheet-icon-button" data-game-toggle-search aria-label="搜索">${icon('search')}</button><button class="ghost compact drama-sheet-button drama-sheet-icon-button" data-game-toggle-filter aria-label="筛选">${icon('sliders')}</button></div><div class="drama-asset-search" hidden><input data-game-asset-search placeholder="搜索${label(kind)}名称" /></div><div class="drama-sheet-list">${assets.length ? assets.map(asset => assetCard(game, asset, rt)).join('') : `<div class="drama-sheet-empty"><div class="empty-icon">${kind === 'character' ? '♙' : kind === 'scene' ? '✦' : '◆'}</div><p>还没有${label(kind)}素材</p><button class="primary drama-sheet-button" data-game-add-asset>${icon('plus')}<span>添加${label(kind)}</span></button></div>`}</div></aside></div>`;
}

/** Open the game material type drawer using the same structure and controls as the short-drama workbench. */
export function openGameAssetDrawer(game: Game, kind: AssetKind, rt: GameMaterialRuntime, refresh: () => Promise<void>, opening = true) {
  void rt.loadVoicePresets();
  closeGameAssetDrawer();
  const wrapper = document.createElement('div');
  wrapper.innerHTML = gameAssetDrawerMarkup(game, kind, rt, opening);
  const sheet = wrapper.firstElementChild as HTMLElement;
  document.body.append(sheet);
  const close = () => sheet.remove();
  const rerender = async (message?: string) => { close(); if (message) rt.toast(message); await refresh(); };
  const rerenderGeneration = () => openGameAssetDrawer(game, kind, rt, refresh, false);
  sheet.addEventListener('click', event => { if (event.target === sheet) close(); });
  sheet.querySelectorAll<HTMLElement>('[data-game-close-sheet]').forEach(button => button.addEventListener('click', close));
  sheet.querySelectorAll<HTMLElement>('[data-game-material-tab]').forEach(button => button.addEventListener('click', () => openGameAssetDrawer(game, button.dataset.gameMaterialTab as AssetKind, rt, refresh, false)));
  sheet.querySelector('[data-game-refresh-assets]')?.addEventListener('click', () => { void rerender('素材已刷新'); });
  sheet.querySelector('[data-game-generate-all-assets]')?.addEventListener('click', async event => {
    const button = event.currentTarget as HTMLButtonElement; button.disabled = true; button.textContent = '正在创建任务…';
    try { const response = await fetch(`${rt.apiBaseUrl}/games/${game.id}/assets/images/batch`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ asset_type: kind }) }); if (!response.ok) throw new Error(await errorMessage(response)); const payload = await response.json() as { tasks?: GameTask[] }; (payload.tasks || []).forEach(task => applyQueuedGameImageTask(game, task)); rt.toast(`已开始生成 ${assetsOf(game, kind).length} 个${label(kind)}素材`); rerenderGeneration(); void refresh(); } catch (error) { button.disabled = false; button.textContent = '生成全部图片'; rt.toast(`批量生成失败：${error instanceof Error ? error.message : '请稍后重试'}`); }
  });
  sheet.querySelectorAll<HTMLElement>('[data-game-add-asset]').forEach(button => button.addEventListener('click', () => openAssetModal(game, kind, undefined, rt, refresh)));
  sheet.querySelectorAll<HTMLElement>('[data-game-edit-asset]').forEach(button => { const asset = assetOf(game, button.dataset.gameEditAsset || ''); if (asset) button.addEventListener('click', () => openAssetModal(game, kind, asset, rt, refresh)); });
  sheet.querySelectorAll<HTMLElement>('[data-game-delete-asset]').forEach(button => button.addEventListener('click', async () => { const asset = assetOf(game, button.dataset.gameDeleteAsset || ''); if (!asset || !window.confirm(`确认删除${label(asset.type)}“${asset.name}”？节点内的引用将被自动清除。`)) return; const response = await fetch(`${rt.apiBaseUrl}/games/${game.id}/assets/${asset.id}`, { method: 'DELETE' }); if (response.ok) await rerender(`${label(asset.type)}已删除`); else rt.toast(`删除失败：${await errorMessage(response)}`); }));
  sheet.querySelectorAll<HTMLElement>('[data-game-generate-asset]').forEach(button => button.addEventListener('click', () => void runTask(game, `/assets/${button.dataset.gameGenerateAsset}/image`, '素材图片任务已创建', rt, refresh, rerenderGeneration)));
  sheet.querySelectorAll<HTMLElement>('[data-game-upload-asset]').forEach(button => { const asset = assetOf(game, button.dataset.gameUploadAsset || ''); if (asset) button.addEventListener('click', () => uploadAsset(game, asset, rt, refresh)); });
  sheet.querySelectorAll<HTMLElement>('[data-game-add-variant],[data-game-change-outfit]').forEach(button => { const asset = assetOf(game, button.dataset.gameAddVariant || button.dataset.gameChangeOutfit || ''); if (asset) button.addEventListener('click', () => openVariantModal(game, asset, undefined, Boolean(button.dataset.gameChangeOutfit), rt, refresh)); });
  sheet.querySelectorAll<HTMLElement>('[data-game-edit-variant]').forEach(button => { const asset = assetOf(game, button.dataset.gameEditVariant || ''); const variant = asset?.variants?.find(item => item.id === button.dataset.gameVariantId); if (asset && variant) button.addEventListener('click', () => openVariantModal(game, asset, variant, false, rt, refresh)); });
  sheet.querySelectorAll<HTMLElement>('[data-game-generate-variant]').forEach(button => button.addEventListener('click', () => void runTask(game, `/assets/${button.dataset.gameGenerateVariant}/variants/${button.dataset.gameVariantId}/image`, '形态图片任务已创建', rt, refresh, rerenderGeneration)));
  sheet.querySelectorAll<HTMLElement>('[data-game-delete-variant]').forEach(button => button.addEventListener('click', async () => { const asset = assetOf(game, button.dataset.gameDeleteVariant || ''); const variant = asset?.variants?.find(item => item.id === button.dataset.gameVariantId); if (!asset || !variant || !window.confirm(`确认删除形态“${variant.name}”？`)) return; const response = await fetch(`${rt.apiBaseUrl}/games/${game.id}/assets/${asset.id}/variants/${variant.id}`, { method: 'DELETE' }); if (response.ok) await rerender('形态已删除'); else rt.toast(`形态删除失败：${await errorMessage(response)}`); }));
  sheet.querySelectorAll<HTMLElement>('[data-game-image-history]').forEach(button => button.addEventListener('click', () => { const asset = assetOf(game, button.dataset.gameParentAsset || ''); const item = asset?.id === button.dataset.gameImageHistory ? asset : asset?.variants?.find(variant => variant.id === button.dataset.gameImageHistory); if (item) openHistoryModal(item, asset?.id === item.id ? item.name : `${asset?.name || '素材'} · ${item.name}`, rt); }));
  sheet.querySelector('[data-game-open-asset-public]')?.addEventListener('click', () => openPublicPromptModal(game, kind, rt, refresh));
  sheet.querySelector('[data-game-toggle-search]')?.addEventListener('click', () => { const field = sheet.querySelector<HTMLElement>('.drama-asset-search'); if (field) { field.hidden = !field.hidden; field.querySelector<HTMLInputElement>('input')?.focus(); } });
  sheet.querySelector<HTMLInputElement>('[data-game-asset-search]')?.addEventListener('input', event => { const query = (event.target as HTMLInputElement).value.trim().toLowerCase(); sheet.querySelectorAll<HTMLElement>('[data-game-asset-card]').forEach(card => { card.hidden = Boolean(query) && !(card.dataset.assetName || '').includes(query); }); });
  sheet.querySelector('[data-game-toggle-filter]')?.addEventListener('click', () => rt.toast('当前素材可按名称搜索，生成状态会实时显示。'));
  sheet.querySelectorAll<HTMLElement>('[data-game-preview-image]').forEach(button => button.addEventListener('click', () => openPreview(button.dataset.gamePreviewImage || '', button.dataset.gamePreviewLabel || '素材', rt)));
}

async function runTask(game: Game, path: string, message: string, rt: GameMaterialRuntime, refresh: () => Promise<void>, rerenderGeneration: () => void) {
  try { const response = await fetch(`${rt.apiBaseUrl}/games/${game.id}${path}`, { method: 'POST' }); if (!response.ok) throw new Error(await errorMessage(response)); applyQueuedGameImageTask(game, await response.json() as GameTask); rt.toast(message); rerenderGeneration(); void refresh(); } catch (error) { rt.toast(`任务创建失败：${error instanceof Error ? error.message : '请稍后重试'}`); }
}

function voiceOptions(rt: GameMaterialRuntime, selected?: string | null) {
  return `<option value="">不设置</option>${rt.getVoicePresets().filter(voice => voice.id !== 'none').map(voice => `<option value="${escape(rt, voice.id)}"${voice.id === selected ? ' selected' : ''}>${escape(rt, voice.name)}</option>`).join('')}`;
}

function openAssetModal(game: Game, kind: AssetKind, asset: GameAsset | undefined, rt: GameMaterialRuntime, refresh: () => Promise<void>) {
  const modal = document.createElement('div'); modal.className = 'modal-backdrop'; const editing = Boolean(asset);
  const voice = kind === 'character' ? `<label>角色音色<select id="game-asset-voice-id">${voiceOptions(rt, asset?.voice_id)}</select><small>支持音频参考的视频模型会随角色参考图附带该音色音源。</small></label>` : '';
  modal.innerHTML = `<div class="modal drama-asset-editor-modal"><button class="close">×</button><div class="modal-head"><h2>${editing ? `编辑${label(kind)}` : `添加${label(kind)}`}</h2><p>保存图片提示词后，可随时生成图片或上传参考图。</p></div><label>${label(kind)}名称<input id="game-asset-name" value="${escape(rt, asset?.name || '')}" autofocus /></label>${voice}<label>图片提示词<textarea id="game-asset-prompt" rows="6">${escape(rt, asset?.prompt || '')}</textarea></label>${editing ? `<label>图片 URL（可选）<input id="game-asset-image-url" value="${escape(rt, asset?.image_url || '')}" /></label>` : ''}<div class="modal-actions"><button class="ghost" data-game-modal-close>取消</button><button class="primary" data-game-asset-save>${editing ? '保存修改' : `添加${label(kind)}`}</button></div></div>`;
  document.body.append(modal); const close = () => modal.remove(); modal.querySelectorAll<HTMLElement>('.close,[data-game-modal-close]').forEach(button => button.addEventListener('click', close));
  modal.querySelector('[data-game-asset-save]')?.addEventListener('click', async event => { const button = event.currentTarget as HTMLButtonElement; const name = modal.querySelector<HTMLInputElement>('#game-asset-name')!.value.trim(); const prompt = modal.querySelector<HTMLTextAreaElement>('#game-asset-prompt')!.value.trim(); const voiceId = modal.querySelector<HTMLSelectElement>('#game-asset-voice-id')?.value || null; if (!name || !prompt) return rt.toast('请填写名称和图片提示词'); button.disabled = true; button.textContent = '保存中…'; try { const url = asset ? `${rt.apiBaseUrl}/games/${game.id}/assets/${asset.id}` : `${rt.apiBaseUrl}/games/${game.id}/assets`; const response = await fetch(url, { method: asset ? 'PUT' : 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(asset ? { name, prompt, voice_id: voiceId, image_url: modal.querySelector<HTMLInputElement>('#game-asset-image-url')?.value.trim() || '' } : { type: kind, name, prompt, voice_id: voiceId }) }); if (!response.ok) throw new Error(await errorMessage(response)); close(); document.querySelector('[data-game-material-sheet]')?.remove(); rt.toast(editing ? '素材已保存' : '素材已添加'); await refresh(); } catch (error) { button.disabled = false; button.textContent = editing ? '保存修改' : `添加${label(kind)}`; rt.toast(`素材保存失败：${error instanceof Error ? error.message : '请稍后重试'}`); } });
}

function openVariantModal(game: Game, asset: GameAsset, variant: GameAssetVariant | undefined, outfit: boolean, rt: GameMaterialRuntime, refresh: () => Promise<void>) {
  const modal = document.createElement('div'); modal.className = 'modal-backdrop'; const editing = Boolean(variant);
  modal.innerHTML = `<div class="modal drama-variant-modal"><button class="close">×</button><div class="modal-head"><h2>${editing ? '编辑形态' : outfit ? '添加换装形态' : '添加形态'}</h2><p>形态会继承${escape(rt, asset.name)}的基础设定，并单独生成、保存图片历史。</p></div><label>形态名称<input id="game-variant-name" value="${escape(rt, variant?.name || (outfit ? '换装形态' : '其他形态'))}" /></label><label>形态图片提示词<textarea id="game-variant-prompt" rows="5">${escape(rt, variant?.prompt || (outfit ? '保持角色身份、脸部、发型与体态一致，仅改变服装搭配。' : ''))}</textarea></label><div class="modal-actions"><button class="ghost" data-game-modal-close>取消</button><button class="primary" data-game-variant-save>${editing ? '保存修改' : '添加形态'}</button></div></div>`;
  document.body.append(modal); const close = () => modal.remove(); modal.querySelectorAll<HTMLElement>('.close,[data-game-modal-close]').forEach(button => button.addEventListener('click', close));
  modal.querySelector('[data-game-variant-save]')?.addEventListener('click', async event => { const button = event.currentTarget as HTMLButtonElement; const name = modal.querySelector<HTMLInputElement>('#game-variant-name')!.value.trim(); const prompt = modal.querySelector<HTMLTextAreaElement>('#game-variant-prompt')!.value.trim(); if (!name || !prompt) return rt.toast('请填写形态名称和提示词'); button.disabled = true; button.textContent = '保存中…'; try { const url = variant ? `${rt.apiBaseUrl}/games/${game.id}/assets/${asset.id}/variants/${variant.id}` : `${rt.apiBaseUrl}/games/${game.id}/assets/${asset.id}/variants`; const response = await fetch(url, { method: variant ? 'PUT' : 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, prompt }) }); if (!response.ok) throw new Error(await errorMessage(response)); close(); document.querySelector('[data-game-material-sheet]')?.remove(); rt.toast(editing ? '形态已保存' : '形态已添加'); await refresh(); } catch (error) { button.disabled = false; button.textContent = editing ? '保存修改' : '添加形态'; rt.toast(`形态保存失败：${error instanceof Error ? error.message : '请稍后重试'}`); } });
}

function uploadAsset(game: Game, asset: GameAsset, rt: GameMaterialRuntime, refresh: () => Promise<void>) {
  const input = document.createElement('input'); input.type = 'file'; input.accept = 'image/png,image/jpeg,image/webp'; input.addEventListener('change', () => { const file = input.files?.[0]; if (!file) return; const reader = new FileReader(); reader.onload = async () => { try { const response = await fetch(`${rt.apiBaseUrl}/games/${game.id}/assets/${asset.id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name: asset.name, prompt: asset.prompt, image_url: String(reader.result || '') }) }); if (!response.ok) throw new Error(await errorMessage(response)); document.querySelector('[data-game-material-sheet]')?.remove(); rt.toast(`${label(asset.type)}图片已上传`); await refresh(); } catch (error) { rt.toast(`图片上传失败：${error instanceof Error ? error.message : '请稍后重试'}`); } }; reader.readAsDataURL(file); }); input.click();
}

function openPublicPromptModal(game: Game, kind: AssetKind, rt: GameMaterialRuntime, refresh: () => Promise<void>) {
  const modal = document.createElement('div'); modal.className = 'modal-backdrop asset-prompt-modal-backdrop'; const defaultPrompt = gameAssetPublicPromptDefault(game, kind);
  modal.innerHTML = `<div class="modal video-prompt-modal asset-prompt-modal"><button class="close">×</button><div class="modal-head"><h2>${label(kind)}公共提示词</h2><p>每次生成${label(kind)}图片时都会统一追加，可按需要覆盖系统默认规范。</p></div><div class="video-prompt-body"><textarea id="game-public-prompt" rows="5" autofocus>${escape(rt, gameAssetPublicPrompt(game, kind))}</textarea></div><div class="video-prompt-actions"><button class="ghost" data-game-public-default>↶&nbsp; 恢复默认</button><button class="ghost" data-game-modal-close>取消</button><button class="primary" data-game-public-save>保存</button></div></div>`;
  document.body.append(modal); const close = () => modal.remove(); modal.querySelectorAll<HTMLElement>('.close,[data-game-modal-close]').forEach(button => button.addEventListener('click', close));
  modal.querySelector('[data-game-public-default]')?.addEventListener('click', () => { const field = modal.querySelector<HTMLTextAreaElement>('#game-public-prompt'); if (field) field.value = defaultPrompt; });
  modal.querySelector('[data-game-public-save]')?.addEventListener('click', async event => { const button = event.currentTarget as HTMLButtonElement; button.disabled = true; try { const response = await fetch(`${rt.apiBaseUrl}/games/${game.id}/asset-public-prompt`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ asset_type: kind, public_prompt: modal.querySelector<HTMLTextAreaElement>('#game-public-prompt')!.value }) }); if (!response.ok) throw new Error(await errorMessage(response)); close(); document.querySelector('[data-game-material-sheet]')?.remove(); rt.toast(`${label(kind)}公共提示词已保存`); await refresh(); } catch (error) { button.disabled = false; rt.toast(`公共提示词保存失败：${error instanceof Error ? error.message : '请稍后重试'}`); } });
}

function openHistoryModal(item: AssetItem, title: string, rt: GameMaterialRuntime) {
  const history = item.image_history || []; const modal = document.createElement('div'); modal.className = 'modal-backdrop drama-image-history-backdrop';
  modal.innerHTML = `<div class="modal drama-image-history-modal"><button class="close">×</button><div class="modal-head"><h2>${escape(rt, title)} · 图片历史</h2><p>保留每次生成的图片记录。</p></div><div class="drama-image-history-grid">${history.map((entry: GameAssetImageHistory, index) => `<article class="drama-image-history-item"><div>${entry.url ? `<img src="${escape(rt, rt.resolveMediaUrl(entry.url))}" alt="${escape(rt, title)}" />` : '图片不可用'}</div><span>第 ${history.length - index} 次</span><small>${escape(rt, entry.generated_at || '')}</small></article>`).join('') || '<p class="hint">暂无图片历史</p>'}</div></div>`;
  document.body.append(modal); modal.querySelector('.close')?.addEventListener('click', () => modal.remove());
}

function openPreview(url: string, title: string, rt: GameMaterialRuntime) {
  const modal = document.createElement('div'); modal.className = 'modal-backdrop game-image-preview-backdrop'; modal.innerHTML = `<div class="modal game-image-preview-modal"><button class="close">×</button><h2>${escape(rt, title)}</h2><img src="${escape(rt, url)}" alt="${escape(rt, title)}" /></div>`; document.body.append(modal); modal.querySelector('.close')?.addEventListener('click', () => modal.remove());
}
