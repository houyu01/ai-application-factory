/** Interactive-game material rail, drama-style drawers, and node media dialogs. */

import type { Game, GameAsset, GameNode, GameTask, VoicePreset } from './models.js';
import { icon } from './ui_icons.js';
import { gameAssetKinds as assetKinds, gameMaterialLabel, gameMaterialRailItems as railItems, gameMaterialRailMarkup } from './game_material_rail.js';
import { openGameAssetDrawer } from './game_asset_drawer_ui.js';
import { openGameCoverModal } from './game_cover_ui.js';
import { openGamePlaceholderModal as openGamePlaceholderEditor } from './game_placeholder_ui.js';
import { bindGameRichPromptEditor, gamePromptReferenceAssetIds, readGamePromptNodes, renderGamePromptNodes, serializeGamePromptNodes } from './game_prompt_rich.js';
import { gameReferenceNode, gameReferencePanelMarkup, openGameReferencePicker } from './game_reference_picker.js';
import { syncGameNodeVideoCancellation } from './game_node_video_cancellation.js';
import { selectedGameNodeVideoUrl } from './game_node_video_history.js';
import { syncGameNodeVideoHistory } from './game_node_video_history_ui.js';
import './game_node_prompt.css';

export type GameMaterialRuntime = {
  apiBaseUrl: string;
  escapeHtml: (value: unknown) => string;
  resolveMediaUrl: (value?: string | null) => string;
  toast: (message: string) => void;
  setGenerationButtonLoading: (button: HTMLButtonElement, loading: boolean, idleText: string) => void;
  getVoicePresets: () => VoicePreset[];
  loadVoicePresets: () => Promise<void>;
};
type Runtime = GameMaterialRuntime;
type TaskFinder = (game: Game, type: string, resourceId?: string) => GameTask | undefined;
type AssetKind = typeof assetKinds[number]['type'];
type RailKind = typeof railItems[number]['type'];

let lastSelectedNodeId: string | null = null;

const errorMessage = async (response: Response) => {
  const body = await response.json().catch(() => null) as { detail?: unknown; message?: unknown } | null;
  return typeof body?.detail === 'string' ? body.detail : typeof body?.message === 'string' ? body.message : `HTTP ${response.status}`;
};
const labelFor = gameMaterialLabel;
const assetFor = (game: Game, id?: string | null) => game.assets?.find(asset => asset.id === id);
const nodeFor = (game: Game, id?: string | null) => game.nodes?.find(node => node.id === id);
const escape = (rt: Runtime, value: unknown) => rt.escapeHtml(value);
const nodeVideoDuration = (value: unknown) => Math.min(15, Math.max(4, Number(value) || 10));

export { gameMaterialRailMarkup };

/** Bind graph nodes plus every material-rail entry to the matching game workbench overlay. */
export function bindGameMaterialInteractions(game: Game, rt: Runtime, findTask: TaskFinder, refresh: () => Promise<void>, bindNodes = true) {
  if (bindNodes) document.querySelectorAll<HTMLElement>('[data-game-node]').forEach(element => element.addEventListener('click', () => selectGameNode(game, element.dataset.gameNode || '', rt, findTask, refresh)));
  document.querySelectorAll<HTMLElement>('[data-game-open-material]').forEach(button => button.addEventListener('click', () => {
    const kind = button.dataset.gameOpenMaterial as RailKind;
    if (assetKinds.some(item => item.type === kind)) openGameAssetDrawer(game, kind as AssetKind, rt, refresh);
    else if (kind === 'frames') openFramesModal(game, rt, refresh);
    else if (kind === 'placeholder') openGamePlaceholderEditor(game, rt, refresh);
    else openGameCoverModal(game, rt, refresh);
  }));
}

/** Read the full node configuration from an active inspector before a toolbar save or node-video task. */
export function gameNodeFormPayload(inspector: HTMLElement) {
  const value = (selector: string) => (inspector.querySelector(selector) as HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement).value;
  const prompt = inspector.querySelector<HTMLTextAreaElement>('#node-prompt');
  const promptRich = prompt?.dataset.promptRich;
  const references = inspector.dataset.gameReferenceAssetIds;
  return {
    title: value('#node-title'), original_text: value('#node-original'), prompt: value('#node-prompt'), duration_seconds: Number(value('#node-duration')),
    prompt_rich: promptRich ? JSON.parse(promptRich) : [],
    reference_asset_ids: references ? JSON.parse(references) : [],
  };
}

/** Retain the game-toolbar save contract for any inspector that still exposes an editable asset. */
export function gameAssetFormPayload(inspector: HTMLElement) {
  const value = (selector: string) => (inspector.querySelector(selector) as HTMLInputElement | HTMLTextAreaElement).value;
  return { name: value('#asset-name'), prompt: value('#asset-prompt'), image_url: value('#asset-image-url') };
}

function nodeVideoUrl(node: GameNode) {
  return selectedGameNodeVideoUrl(node);
}

function nodeVideoPreviewMarkup(node: GameNode, rt: Runtime) {
  const selectedUrl = nodeVideoUrl(node);
  const player = selectedUrl
    ? `<video class="game-node-video-player" controls playsinline src="${escape(rt, rt.resolveMediaUrl(selectedUrl))}"></video>`
    : '<div class="game-node-video-placeholder"><div>✦</div><strong>生成视频后将在这里预览</strong><span>每次生成都会保留历史版本，可在下方切换。</span></div>';
  return `<section class="game-node-video-preview"><div class="game-node-media-heading"><button class="primary compact" id="node-generate">生成节点视频</button><div class="game-node-preview-title"><div><h3>视频预览</h3><span class="status ${node.status === '生成中' ? 'running' : ''}">${escape(rt, node.status)}</span></div></div></div>${player}<div class="game-node-video-history" data-game-node-video-history></div></section>`;
}

function nodeInspectorMarkup(node: GameNode, rt: Runtime) {
  return `<div class="inspector-head"><input id="node-title" class="game-node-title-input" value="${escape(rt, node.title)}" aria-label="节点标题" /><div class="game-node-head-actions"><span class="status ${node.status === '生成中' ? 'running' : ''}">${escape(rt, node.status)}</span></div></div><label>原始文本<textarea id="node-original" rows="4">${escape(rt, node.original_text)}</textarea></label><div class="game-node-media-workspace">${nodeVideoPreviewMarkup(node, rt)}<section class="game-node-prompt-panel"><div class="game-node-media-heading"><div><h3>视频提示词</h3><p>可输入 @ 或点击下方参考图，在光标位置插入图片引用。</p></div></div><div class="drama-rich-prompt-toolbar" data-game-prompt-toolbar></div><div class="game-rich-prompt-frame drama-rich-prompt-frame"><div class="game-rich-prompt-editor drama-rich-prompt-editor" contenteditable="true" role="textbox" aria-label="视频提示词"></div></div><textarea id="node-prompt" hidden>${escape(rt, node.prompt)}</textarea><div data-game-reference-panel></div></section></div><label>视频时长（秒）<input id="node-duration" type="number" min="4" max="15" value="${nodeVideoDuration(node.duration_seconds)}" /></label>`;
}

/** Open the shared node inspector after either a canvas click or a material shortcut. */
export function selectGameNode(game: Game, nodeId: string, rt: Runtime, findTask: TaskFinder, refresh: () => Promise<void>) {
  const node = nodeFor(game, nodeId);
  const inspector = document.querySelector<HTMLElement>('#game-inspector');
  if (!node || !inspector) return;
  lastSelectedNodeId = nodeId;
  inspector.dataset.gameSelected = `node:${nodeId}`;
  const task = findTask(game, 'node_video_generation', nodeId);
  let referenceIds = [...new Set(node.reference_asset_ids || [])];
  inspector.innerHTML = nodeInspectorMarkup(node, rt);
  const syncHistory = (currentTask = task) => syncGameNodeVideoHistory({ apiBaseUrl: rt.apiBaseUrl, game, inspector, node, resolveMediaUrl: rt.resolveMediaUrl, task: currentTask, toast: rt.toast, refresh });
  syncHistory();
  const syncReferencePanel = () => {
    const panel = inspector.querySelector<HTMLElement>('[data-game-reference-panel]');
    if (!panel) return;
    inspector.dataset.gameReferenceAssetIds = JSON.stringify(referenceIds);
    node.reference_asset_ids = referenceIds;
    panel.innerHTML = gameReferencePanelMarkup(game, referenceIds, rt);
    panel.querySelector('[data-game-add-reference]')?.addEventListener('click', () => openGameReferencePicker(game, referenceIds, rt, ids => { referenceIds = ids; syncReferencePanel(); }));
    panel.querySelectorAll<HTMLElement>('[data-game-remove-reference]').forEach(button => button.addEventListener('click', () => {
      const id = button.dataset.gameRemoveReference || '';
      referenceIds = referenceIds.filter(item => item !== id);
      const editor = inspector.querySelector<HTMLElement>('.game-rich-prompt-editor');
      if (editor) {
        const kept = readGamePromptNodes(editor).filter(item => item.type !== 'reference' || item.asset_id !== id);
        renderGamePromptNodes(editor, game, rt, kept);
        const serialized = serializeGamePromptNodes(game, kept);
        const source = inspector.querySelector<HTMLTextAreaElement>('#node-prompt');
        if (source) { source.value = serialized.prompt; source.dataset.promptRich = JSON.stringify(serialized.nodes); }
        node.prompt = serialized.prompt; node.prompt_rich = serialized.nodes;
      }
      syncReferencePanel();
    }));
  };
  syncReferencePanel();
  bindGameRichPromptEditor({
    inspector, game, node, runtime: rt,
    onUpdate: serialized => {
      node.prompt = serialized.prompt;
      node.prompt_rich = serialized.nodes;
      const updated = [...new Set([...referenceIds, ...gamePromptReferenceAssetIds(serialized.nodes)])];
      if (updated.length !== referenceIds.length) { referenceIds = updated; syncReferencePanel(); }
    },
    openMentionPicker: onComplete => openGameReferencePicker(game, referenceIds, rt, ids => {
      const item = gameReferenceNode(game, ids[0] || '');
      if (item) onComplete(item);
    }, true),
  });
  const generate = inspector.querySelector<HTMLButtonElement>('#node-generate');
  const syncVideoCancellation = (currentTask = task) => syncGameNodeVideoCancellation({ apiBaseUrl: rt.apiBaseUrl, game, node, task: currentTask, toast: rt.toast, onCancelled: refresh });
  const setGeneratingState = (queuedTask: GameTask) => {
    node.status = '生成中';
    game.tasks = [...(game.tasks || []).filter(item => item.id !== queuedTask.id), queuedTask];
    inspector.querySelectorAll<HTMLElement>('.status').forEach(status => { status.classList.add('running'); status.textContent = '生成中'; });
    if (generate) { rt.setGenerationButtonLoading(generate, true, '生成节点视频'); generate.textContent = `⟳ 生成节点视频 ${queuedTask.progress || 0}%`; }
    syncHistory(queuedTask);
    syncVideoCancellation(queuedTask);
  };
  if (generate) { rt.setGenerationButtonLoading(generate, Boolean(task?.status === '生成中' || node.status === '生成中'), '生成节点视频'); if (task?.status === '生成中') generate.textContent = `⟳ 生成节点视频 ${task.progress || 0}%`; }
  syncVideoCancellation();
  generate?.addEventListener('click', async () => { try { const response = await fetch(`${rt.apiBaseUrl}/games/${game.id}/nodes/${nodeId}/video`, { method: 'POST' }); if (!response.ok) throw new Error(await errorMessage(response)); setGeneratingState(await response.json() as GameTask); rt.toast('节点视频任务已创建，已使用右上角保存的提示词与参考图配置'); await refresh(); } catch (error) { if (generate) rt.setGenerationButtonLoading(generate, false, '生成节点视频'); rt.toast(`节点视频任务创建失败：${error instanceof Error ? error.message : '请稍后重试'}`); } });
}

function assetCard(game: Game, asset: GameAsset, rt: Runtime) {
  const image = asset.image_url ? `<img src="${escape(rt, asset.image_url)}" alt="${escape(rt, asset.name)}" />` : railItems.find(item => item.type === asset.type)?.icon || '◇';
  return `<article class="drama-asset-card game-material-card" data-game-asset-card="${escape(rt, asset.id)}"><div class="drama-asset-main"><div class="drama-asset-image"><div class="drama-asset-placeholder">${image}</div></div><div class="drama-asset-body"><div class="drama-asset-heading"><div><h3>${escape(rt, asset.name)}</h3><span>${escape(rt, labelFor(asset.type))} · 基础素材</span></div><span class="status">${escape(rt, asset.status)}</span></div><p class="drama-asset-prompt"><b>图片提示词：</b>${escape(rt, asset.prompt || '等待素材提示词')}</p><p class="game-material-reference-state">${asset.image_url ? '参考图已配置' : '尚未配置参考图'}</p></div></div><div class="drama-asset-actions"><button class="ghost compact" data-game-edit-asset="${escape(rt, asset.id)}">${icon('edit')}<span>编辑素材</span></button><button class="ghost compact" data-game-configure-image="${escape(rt, asset.id)}">${icon('image')}<span>参考图配置</span></button></div></article>`;
}

function drawerMarkup(game: Game, kind: AssetKind, rt: Runtime) {
  const assets = (game.assets || []).filter(asset => asset.type === kind);
  return `<div class="drama-sheet-backdrop game-material-sheet-backdrop" data-game-material-sheet><aside class="drama-asset-sheet game-material-sheet"><div class="drama-sheet-head"><div><div class="eyebrow">素材库 / ${labelFor(kind)}</div><h2>${labelFor(kind)}素材 <span class="sheet-badge">人工配置</span></h2><p>共 ${assets.length} 个素材；提示词和参考图均可在此维护。</p></div><button class="close sheet-close" data-game-close-sheet aria-label="关闭">×</button></div><div class="drama-sheet-tabs">${assetKinds.map(item => `<button class="${item.type === kind ? 'active' : ''}" data-game-material-tab="${item.type}">${item.label} <small>${(game.assets || []).filter(asset => asset.type === item.type).length}</small></button>`).join('')}</div><div class="drama-sheet-toolbar drama-sheet-toolbar-primary"><span class="game-material-toolbar-copy">图片由人工上传或填写参考图 URL，不会自动发起生图任务。</span></div><div class="drama-sheet-toolbar drama-sheet-toolbar-secondary"><button class="ghost drama-sheet-button" data-game-refresh-assets>${icon('refresh')}<span>刷新</span></button><span class="drama-sheet-toolbar-spacer"></span><button class="ghost compact drama-sheet-button" data-game-close-sheet>${icon('collapse')}<span>收起</span></button></div><div class="drama-sheet-list">${assets.length ? assets.map(asset => assetCard(game, asset, rt)).join('') : `<div class="drama-sheet-empty"><div class="empty-icon">${assetKinds.find(item => item.type === kind)?.icon || '◇'}</div><p>还没有${labelFor(kind)}素材</p></div>`}</div></aside></div>`;
}

function openAssetDrawer(game: Game, kind: AssetKind, rt: Runtime, refresh: () => Promise<void>) {
  document.querySelector('[data-game-material-sheet]')?.remove();
  const wrapper = document.createElement('div');
  wrapper.innerHTML = drawerMarkup(game, kind, rt);
  const sheet = wrapper.firstElementChild as HTMLElement;
  document.body.append(sheet);
  const close = () => sheet.remove();
  sheet.addEventListener('click', event => { if (event.target === sheet) close(); });
  sheet.querySelectorAll<HTMLElement>('[data-game-close-sheet]').forEach(button => button.addEventListener('click', close));
  sheet.querySelectorAll<HTMLElement>('[data-game-material-tab]').forEach(button => button.addEventListener('click', () => openAssetDrawer(game, button.dataset.gameMaterialTab as AssetKind, rt, refresh)));
  sheet.querySelector('[data-game-refresh-assets]')?.addEventListener('click', () => { close(); void refresh(); });
  sheet.querySelectorAll<HTMLElement>('[data-game-edit-asset],[data-game-configure-image]').forEach(button => button.addEventListener('click', () => {
    const asset = assetFor(game, button.dataset.gameEditAsset || button.dataset.gameConfigureImage);
    if (asset) openAssetEditorModal(game, asset, rt, refresh, Boolean(button.dataset.gameConfigureImage));
  }));
}

function openAssetEditorModal(game: Game, asset: GameAsset, rt: Runtime, refresh: () => Promise<void>, imageOnly = false) {
  const modal = document.createElement('div');
  const label = labelFor(asset.type);
  modal.className = 'modal-backdrop game-material-editor-backdrop';
  modal.innerHTML = `<div class="modal drama-asset-editor-modal"><button class="close" aria-label="关闭">×</button><div class="modal-head"><h2>${imageOnly ? `配置${label}参考图` : `编辑${label}`}</h2><p>${imageOnly ? '上传图片或填写 URL，保存后可作为节点视频、首尾帧和占位图参考。' : '修改素材名称、图片提示词和参考图，操作方式与短剧素材一致。'}</p></div>${imageOnly ? '' : `<label>${label}名称<input id="game-asset-name" value="${escape(rt, asset.name)}" /></label><label>图片提示词<textarea id="game-asset-prompt" rows="6">${escape(rt, asset.prompt)}</textarea></label>`}<label>参考图 URL（可选）<input id="game-asset-image-url" type="url" value="${escape(rt, asset.image_url || '')}" placeholder="https://… 或上传本地图片" /></label><label class="ghost compact game-material-upload">${icon('upload')}<span>上传参考图<input type="file" accept="image/*" data-game-asset-upload hidden /></span></label><div class="modal-actions"><button class="ghost" data-game-editor-cancel>取消</button><button class="primary" data-game-editor-save>保存修改</button></div></div>`;
  document.body.append(modal);
  const close = () => modal.remove();
  modal.querySelectorAll<HTMLElement>('.close,[data-game-editor-cancel]').forEach(button => button.addEventListener('click', close));
  modal.querySelector<HTMLInputElement>('[data-game-asset-upload]')?.addEventListener('change', event => {
    const file = (event.target as HTMLInputElement).files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => { const field = modal.querySelector<HTMLInputElement>('#game-asset-image-url'); if (field) field.value = String(reader.result || ''); };
    reader.readAsDataURL(file);
  });
  modal.querySelector('[data-game-editor-save]')?.addEventListener('click', async () => {
    const button = modal.querySelector<HTMLButtonElement>('[data-game-editor-save]')!;
    const name = modal.querySelector<HTMLInputElement>('#game-asset-name')?.value.trim() || asset.name;
    const prompt = modal.querySelector<HTMLTextAreaElement>('#game-asset-prompt')?.value.trim() || asset.prompt;
    const imageUrl = modal.querySelector<HTMLInputElement>('#game-asset-image-url')?.value.trim() || '';
    button.disabled = true; button.textContent = '保存中…';
    try { const response = await fetch(`${rt.apiBaseUrl}/games/${game.id}/assets/${asset.id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, prompt, image_url: imageUrl }) }); if (!response.ok) throw new Error(await errorMessage(response)); close(); document.querySelector('[data-game-material-sheet]')?.remove(); rt.toast(`${label}素材已保存`); await refresh(); }
    catch (error) { button.disabled = false; button.textContent = '保存修改'; rt.toast(`素材保存失败：${error instanceof Error ? error.message : '请稍后重试'}`); }
  });
}

function selectableNode(game: Game, requested?: string | null) { return nodeFor(game, requested || lastSelectedNodeId) || game.nodes?.[0]; }
function nodeOptions(game: Game, selectedId?: string) { return (game.nodes || []).map(node => `<option value="${node.id}"${node.id === selectedId ? ' selected' : ''}>${node.title}</option>`).join(''); }
function referenceOptions(game: Game, selectedId?: string | null) { return `<option value="">不设置</option>${(game.assets || []).filter(asset => !['cover', 'cover_reference'].includes(asset.type)).map(asset => `<option value="${asset.id}"${asset.id === selectedId ? ' selected' : ''}>${asset.name}（${labelFor(asset.type)}）</option>`).join('')}`; }
function imagePreview(game: Game, assetId?: string | null, rt?: Runtime) { const asset = assetFor(game, assetId); return asset?.image_url ? `<img src="${escape(rt!, asset.image_url)}" alt="${escape(rt!, asset.name)}" />` : `<span>${asset ? escape(rt!, asset.name) : '尚未设置'}</span>`; }

function openFramesModal(game: Game, rt: Runtime, refresh: () => Promise<void>, requestedNodeId?: string) {
  const selectedNode = selectableNode(game, requestedNodeId);
  if (!selectedNode) { rt.toast('请等待视频节点生成后再配置首尾帧'); return; }
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop game-frame-backdrop';
  modal.innerHTML = `<div class="modal drama-frame-modal"><button type="button" class="close" aria-label="关闭">×</button><div class="modal-head"><h2>首尾帧 · ${escape(rt, selectedNode.title)}</h2><p>为当前视频节点选择首帧和尾帧，作为节点视频生成时的起止画面参考。</p></div><label class="game-material-node-picker">视频节点<select data-game-frame-node>${nodeOptions(game, selectedNode.id)}</select></label><div class="drama-frame-editor-grid">${(['first', 'last'] as const).map(side => `<section class="drama-frame-editor-card"><h3>${side === 'first' ? '输入首帧' : '输入尾帧'}</h3><div class="drama-frame-preview" data-game-frame-preview="${side}">${imagePreview(game, selectedNode.first_last_frames?.[side]?.asset_id, rt)}</div><div class="drama-frame-actions"><label class="ghost compact">选择${side === 'first' ? '首' : '尾'}帧<select data-game-frame-select="${side}">${referenceOptions(game, selectedNode.first_last_frames?.[side]?.asset_id)}</select></label></div></section>`).join('')}</div><div class="modal-actions"><button type="button" class="ghost" data-game-frame-clear>清除首尾帧</button><button type="button" class="primary" data-game-frame-save>完成</button></div></div>`;
  document.body.append(modal);
  const close = () => modal.remove();
  modal.querySelector('.close')?.addEventListener('click', close);
  const currentNode = () => nodeFor(game, modal.querySelector<HTMLSelectElement>('[data-game-frame-node]')?.value) || selectedNode;
  const sync = () => { const node = currentNode(); modal.querySelectorAll<HTMLSelectElement>('[data-game-frame-select]').forEach(field => { const side = field.dataset.gameFrameSelect as 'first' | 'last'; field.value = node.first_last_frames?.[side]?.asset_id || ''; }); modal.querySelectorAll<HTMLElement>('[data-game-frame-preview]').forEach(preview => { const side = preview.dataset.gameFramePreview as 'first' | 'last'; preview.innerHTML = imagePreview(game, node.first_last_frames?.[side]?.asset_id, rt); }); };
  modal.querySelector('[data-game-frame-node]')?.addEventListener('change', sync);
  modal.querySelectorAll<HTMLSelectElement>('[data-game-frame-select]').forEach(field => field.addEventListener('change', () => { const preview = modal.querySelector<HTMLElement>(`[data-game-frame-preview="${field.dataset.gameFrameSelect}"]`); if (preview) preview.innerHTML = imagePreview(game, field.value, rt); }));
  modal.querySelector('[data-game-frame-clear]')?.addEventListener('click', () => modal.querySelectorAll<HTMLSelectElement>('[data-game-frame-select]').forEach(field => { field.value = ''; field.dispatchEvent(new Event('change')); }));
  modal.querySelector('[data-game-frame-save]')?.addEventListener('click', async event => { const button = event.currentTarget as HTMLButtonElement; const node = currentNode(); const frame = (side: 'first' | 'last') => { const id = modal.querySelector<HTMLSelectElement>(`[data-game-frame-select="${side}"]`)?.value; return id ? { asset_id: id } : null; }; button.disabled = true; button.textContent = '保存中…'; try { const response = await fetch(`${rt.apiBaseUrl}/games/${game.id}/nodes/${node.id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ first_last_frames: { first: frame('first'), last: frame('last') } }) }); if (!response.ok) throw new Error(await errorMessage(response)); close(); rt.toast('首尾帧已保存'); await refresh(); } catch (error) { button.disabled = false; button.textContent = '完成'; rt.toast(`首尾帧保存失败：${error instanceof Error ? error.message : '请稍后重试'}`); } });
}

function openPlaceholderModal(game: Game, rt: Runtime, refresh: () => Promise<void>, requestedNodeId?: string) {
  const selectedNode = selectableNode(game, requestedNodeId);
  if (!selectedNode) { rt.toast('请等待视频节点生成后再配置占位图'); return; }
  const placeholders = (game.assets || []).filter(asset => asset.type === 'placeholder');
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop drama-placeholder-backdrop game-placeholder-backdrop';
  modal.innerHTML = `<div class="modal drama-placeholder-modal"><div class="modal-head"><button class="close" aria-label="关闭">×</button><h2>占位图 · ${escape(rt, selectedNode.title)}</h2><p>选择节点构图占位图；它会与参考素材一起传入该节点的视频生成任务。</p></div><div class="drama-placeholder-body"><div class="drama-placeholder-main"><section class="drama-placeholder-canvas-card"><div class="drama-placeholder-canvas-head"><span>当前节点</span><select data-game-placeholder-node>${nodeOptions(game, selectedNode.id)}</select></div><div class="drama-placeholder-canvas landscape" data-game-placeholder-preview>${imagePreview(game, selectedNode.placeholder_asset_id, rt)}</div><p class="drama-placeholder-hint">占位图用于固定本节点的构图意图。可先在素材栏上传一张草图或构图参考。</p></section></div><aside class="drama-placeholder-side"><section class="drama-placeholder-section"><div class="section-title"><div><h3>选择占位图</h3><p>与短剧占位图入口一致</p></div><span>${placeholders.length} 个</span></div><div class="drama-placeholder-scene-list"><button type="button" class="drama-placeholder-scene-option" data-game-placeholder-option=""><span>×</span><b>不使用占位图</b></button>${placeholders.map(asset => `<button type="button" class="drama-placeholder-scene-option" data-game-placeholder-option="${escape(rt, asset.id)}"><span>${asset.image_url ? `<img src="${escape(rt, asset.image_url)}" alt="" />` : '<i>▱</i>'}</span><b>${escape(rt, asset.name)}</b></button>`).join('') || '<div class="drama-placeholder-empty">暂无占位图素材</div>'}</div></section></aside></div><div class="modal-actions"><button class="ghost" data-game-placeholder-clear>清除</button><button class="primary" data-game-placeholder-save>完成</button></div></div>`;
  document.body.append(modal);
  let selectedAssetId = selectedNode.placeholder_asset_id || '';
  const close = () => modal.remove();
  const currentNode = () => nodeFor(game, modal.querySelector<HTMLSelectElement>('[data-game-placeholder-node]')?.value) || selectedNode;
  const update = () => { const node = currentNode(); if (node.id !== selectedNode.id) selectedAssetId = node.placeholder_asset_id || ''; modal.querySelector<HTMLElement>('[data-game-placeholder-preview]')!.innerHTML = imagePreview(game, selectedAssetId, rt); modal.querySelectorAll<HTMLElement>('[data-game-placeholder-option]').forEach(option => option.classList.toggle('selected', (option.dataset.gamePlaceholderOption || '') === selectedAssetId)); };
  modal.querySelector('.close')?.addEventListener('click', close);
  modal.querySelector('[data-game-placeholder-node]')?.addEventListener('change', update);
  modal.querySelectorAll<HTMLElement>('[data-game-placeholder-option]').forEach(option => option.addEventListener('click', () => { selectedAssetId = option.dataset.gamePlaceholderOption || ''; update(); }));
  modal.querySelector('[data-game-placeholder-clear]')?.addEventListener('click', () => { selectedAssetId = ''; update(); });
  modal.querySelector('[data-game-placeholder-save]')?.addEventListener('click', async event => { const button = event.currentTarget as HTMLButtonElement; const node = currentNode(); button.disabled = true; button.textContent = '保存中…'; try { const response = await fetch(`${rt.apiBaseUrl}/games/${game.id}/nodes/${node.id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ placeholder_asset_id: selectedAssetId || null }) }); if (!response.ok) throw new Error(await errorMessage(response)); close(); rt.toast('占位图已保存'); await refresh(); } catch (error) { button.disabled = false; button.textContent = '完成'; rt.toast(`占位图保存失败：${error instanceof Error ? error.message : '请稍后重试'}`); } });
  update();
}
