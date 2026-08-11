/** Interactive-game counterpart of the drama placeholder layout editor and composite-image workflow. */

import type { Game, GameAsset, GameNode, GamePlaceholderPlacement, GameTask } from './models.js';
import { icon } from './ui_icons.js';

type Runtime = {
  apiBaseUrl: string;
  escapeHtml: (value: unknown) => string;
  resolveMediaUrl: (value?: string | null) => string;
  toast: (message: string) => void;
};
type PlaceholderState = {
  game: Game;
  rt: Runtime;
  refresh: () => Promise<void>;
  modal: HTMLElement;
  nodeId: string;
  sceneId: string;
  placements: GamePlaceholderPlacement[];
  render: () => void;
};

let activeState: PlaceholderState | null = null;

const escape = (rt: Runtime, value: unknown) => rt.escapeHtml(value);
const asset = (game: Game, id?: string | null) => game.assets?.find(item => item.id === id);
const node = (game: Game, id?: string | null) => game.nodes?.find(item => item.id === id);
const readyAssets = (game: Game, type: string) => (game.assets || []).filter(item => item.type === type && Boolean(item.image_url));
const ratio = (game: Game) => game.platform === 'Steam游戏' ? 'landscape' : 'portrait';

function normalize(placement: Partial<GamePlaceholderPlacement>, index: number): GamePlaceholderPlacement {
  const width = Math.min(1, Math.max(0.04, Number(placement.width) || 0.2));
  const height = Math.min(1, Math.max(0.04, Number(placement.height) || 0.35));
  const x = Math.min(1 - width, Math.max(0, Number.isFinite(Number(placement.x)) ? Number(placement.x) : Math.min(0.72, 0.28 + index * 0.16)));
  const y = Math.min(1 - height, Math.max(0, Number.isFinite(Number(placement.y)) ? Number(placement.y) : Math.min(0.62, 0.26 + index * 0.08)));
  return { id: placement.id || `game-placement-${Date.now()}-${index}`, asset_id: placement.asset_id || '', x, y, width, height, pose: placement.pose || '', note: placement.note || placement.pose || '' };
}

function nodeOptions(game: Game, selectedId: string, rt: Runtime) {
  return (game.nodes || []).map(item => `<option value="${escape(rt, item.id)}"${item.id === selectedId ? ' selected' : ''}>${escape(rt, item.title)}</option>`).join('');
}

function history(game: Game, nodeId: string) {
  return (game.assets || []).filter(item => item.type === 'placeholder' && item.metadata?.node_id === nodeId && item.metadata?.render_mode === 'generated_composite').sort((left, right) => String(right.updated_at || '').localeCompare(String(left.updated_at || '')));
}

function taskFor(game: Game, nodeId: string) {
  return [...(game.tasks || [])].reverse().find(task => task.type === 'game_placeholder_image' && task.input_snapshot?.node_id === nodeId);
}

function updateNode(game: Game, updated: GameNode) {
  const index = game.nodes?.findIndex(item => item.id === updated.id) ?? -1;
  if (index >= 0 && game.nodes) game.nodes.splice(index, 1, updated);
}

async function responseError(response: Response) {
  const body = await response.json().catch(() => null) as { detail?: unknown; message?: unknown } | null;
  return typeof body?.detail === 'string' ? body.detail : typeof body?.message === 'string' ? body.message : `HTTP ${response.status}`;
}

function selectedNode(state: PlaceholderState) {
  return node(state.game, state.nodeId);
}

function close(state: PlaceholderState) {
  state.modal.remove();
  if (activeState === state) activeState = null;
}

function selectionPayload(state: PlaceholderState) {
  return { node_id: state.nodeId, scene_asset_id: state.sceneId, placements: state.placements };
}

async function saveDraft(state: PlaceholderState) {
  if (!state.sceneId) throw new Error('请先选择已生成的场景图');
  const response = await fetch(`${state.rt.apiBaseUrl}/games/${state.game.id}/nodes/${state.nodeId}/placeholder-layout`, {
    method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(selectionPayload(state)),
  });
  if (!response.ok) throw new Error(await responseError(response));
  updateNode(state.game, await response.json() as GameNode);
}

function bindDrag(state: PlaceholderState) {
  state.modal.querySelectorAll<HTMLElement>('[data-game-placeholder-drag]').forEach(box => {
    box.addEventListener('pointerdown', event => {
      const placement = state.placements.find(item => item.id === box.dataset.gamePlaceholderDrag);
      const canvas = state.modal.querySelector<HTMLElement>('[data-game-placeholder-canvas]');
      if (!placement || !canvas) return;
      const bounds = canvas.getBoundingClientRect();
      const origin = { x: event.clientX, y: event.clientY, left: placement.x, top: placement.y };
      box.setPointerCapture(event.pointerId);
      const move = (pointer: PointerEvent) => {
        placement.x = Math.min(1 - placement.width, Math.max(0, origin.left + (pointer.clientX - origin.x) / bounds.width));
        placement.y = Math.min(1 - placement.height, Math.max(0, origin.top + (pointer.clientY - origin.y) / bounds.height));
        box.style.left = `${placement.x * 100}%`;
        box.style.top = `${placement.y * 100}%`;
      };
      const finish = () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', finish); };
      window.addEventListener('pointermove', move);
      window.addEventListener('pointerup', finish);
    });
  });
}

function render(state: PlaceholderState) {
  const current = selectedNode(state);
  if (!current) return close(state);
  const scenes = readyAssets(state.game, 'scene');
  const characters = readyAssets(state.game, 'character');
  if (!state.sceneId && scenes[0]) state.sceneId = scenes[0].id;
  const scene = asset(state.game, state.sceneId);
  const versions = history(state.game, current.id);
  const task = taskFor(state.game, current.id);
  const running = task?.status === '生成中';
  const failed = task?.status === '生成失败' ? task.error_message : '';
  const preview = scene?.image_url
    ? `<img src="${escape(state.rt, state.rt.resolveMediaUrl(scene.image_url))}" data-drama-image-preview="${escape(state.rt, state.rt.resolveMediaUrl(scene.image_url))}" data-drama-image-label="${escape(state.rt, scene.name)}" alt="${escape(state.rt, scene.name)}" />`
    : '<div class="drama-placeholder-canvas-empty">请先选择一张已生成的场景图片</div>';
  const boxes = state.placements.map((placement, index) => {
    const role = asset(state.game, placement.asset_id);
    return `<button type="button" class="drama-placeholder-box" data-game-placeholder-drag="${escape(state.rt, placement.id)}" style="left:${placement.x * 100}%;top:${placement.y * 100}%;width:${placement.width * 100}%;height:${placement.height * 100}%"><b>${String.fromCharCode(65 + index % 26)}</b><span>${escape(state.rt, role?.name || '角色')}</span></button>`;
  }).join('');
  const sceneList = scenes.length ? `<div class="drama-placeholder-scene-list">${scenes.map(item => `<button type="button" class="drama-placeholder-scene-option ${item.id === state.sceneId ? 'selected' : ''}" data-game-placeholder-scene="${escape(state.rt, item.id)}"><span><img src="${escape(state.rt, state.rt.resolveMediaUrl(item.image_url))}" data-drama-image-preview="${escape(state.rt, state.rt.resolveMediaUrl(item.image_url))}" data-drama-image-label="${escape(state.rt, item.name)}" alt="" /></span><b>${escape(state.rt, item.name)}</b></button>`).join('')}</div>` : '<div class="drama-placeholder-empty">当前还没有已生成的场景图。</div>';
  const characterList = characters.length ? `<div class="drama-placeholder-role-list">${characters.map(item => { const count = state.placements.filter(placement => placement.asset_id === item.id).length; return `<button type="button" class="drama-placeholder-role-option" data-game-placeholder-add-role="${escape(state.rt, item.id)}"><span><img src="${escape(state.rt, state.rt.resolveMediaUrl(item.image_url))}" data-drama-image-preview="${escape(state.rt, state.rt.resolveMediaUrl(item.image_url))}" data-drama-image-label="${escape(state.rt, item.name)}" alt="" /></span><div><b>${escape(state.rt, item.name)}</b><small>${count ? `已放置 ${count} 个` : '添加到占位图'}</small></div><strong>＋</strong></button>`; }).join('')}</div>` : '<div class="drama-placeholder-empty">当前还没有已生成的角色图。</div>';
  const placementList = state.placements.length ? `<div class="drama-placeholder-placement-list">${state.placements.map((placement, index) => { const role = asset(state.game, placement.asset_id); return `<div class="drama-placeholder-placement-item"><div><b>${String.fromCharCode(65 + index % 26)} · ${escape(state.rt, role?.name || '角色')}</b><button type="button" class="drama-placeholder-remove" data-game-placeholder-remove="${escape(state.rt, placement.id)}" aria-label="删除该角色" title="删除">${icon('trash')}</button></div><input data-game-placeholder-note="${escape(state.rt, placement.id)}" value="${escape(state.rt, placement.note || placement.pose || '')}" placeholder="动作或位置备注" /></div>`; }).join('')}</div>` : '<div class="drama-placeholder-empty">还没有占位框，请从上方角色列表添加。</div>';
  const historyMarkup = versions.length ? `<div class="drama-placeholder-history"><div class="section-title"><div><h3>占位图历史</h3><p>每次生成都会保留一个版本。</p></div><span>${versions.length} 个版本</span></div><div class="drama-placeholder-history-grid">${versions.map((item, index) => `<div class="drama-placeholder-history-card">${item.image_url ? `<button type="button" class="drama-placeholder-image-preview" data-drama-image-preview="${escape(state.rt, state.rt.resolveMediaUrl(item.image_url))}" data-drama-image-label="${escape(state.rt, item.name)}"><img src="${escape(state.rt, state.rt.resolveMediaUrl(item.image_url))}" alt="${escape(state.rt, item.name)}" /></button>` : '<div class="drama-placeholder-history-empty">生成中…</div>'}<small>版本 ${item.metadata?.version || versions.length - index} · ${escape(state.rt, item.status || '未生成')}</small></div>`).join('')}</div></div>` : '';
  state.modal.innerHTML = `<div class="modal drama-placeholder-modal"><div class="modal-head"><button class="close" data-game-placeholder-close aria-label="关闭">×</button><h2>占位图</h2><p>为当前视频节点设置角色在场景中的相对位置，生成后可作为节点视频参考图。</p></div><div class="drama-placeholder-body"><div class="drama-placeholder-main"><div class="drama-placeholder-canvas-card"><div class="drama-placeholder-canvas-head"><select data-game-placeholder-node>${nodeOptions(state.game, current.id, state.rt)}</select><span>已放置 ${state.placements.length} 个角色</span></div><div class="drama-placeholder-canvas ${ratio(state.game)}" data-game-placeholder-canvas>${preview}${boxes}</div><p class="drama-placeholder-hint">橙色框和字母只用于编辑构图草稿；生成时会结合场景、角色和相关道具图片，生成无框、无标记的干净参考图。</p></div>${historyMarkup}${failed ? `<p class="drama-cover-error">${escape(state.rt, failed)}</p>` : ''}</div><div class="drama-placeholder-side"><section class="drama-placeholder-section"><div class="section-title"><div><h3>场景</h3><p>选择已生成的场景作为背景。</p></div><span>${scenes.length} 个可用</span></div>${sceneList}</section><section class="drama-placeholder-section"><div class="section-title"><div><h3>角色</h3><p>点击角色添加到场景中。</p></div><span>${characters.length} 个可用</span></div>${characterList}</section><section class="drama-placeholder-section"><div class="section-title"><div><h3>占位框</h3><p>可以删除角色或补充动作备注。</p></div><button type="button" class="ghost compact" data-game-placeholder-clear ${state.placements.length ? '' : 'disabled'}>清空</button></div>${placementList}</section></div></div><div class="modal-actions"><button type="button" class="ghost" data-game-placeholder-cancel>取消</button><button type="button" class="ghost" data-game-placeholder-save>保存草稿</button><button type="button" class="primary${running ? ' is-loading' : ''}" data-game-placeholder-generate${state.sceneId && state.placements.length && !running ? '' : ' disabled'}>${running ? `<span class="generation-spinner" aria-hidden="true"></span><span>生成中... ${task?.progress || 0}%</span>` : `${icon('image')}<span>${failed ? '重新生成占位图' : '生成占位图'}</span>`}</button></div></div>`;
  state.modal.querySelector('[data-game-placeholder-close]')?.addEventListener('click', () => close(state));
  state.modal.querySelector('[data-game-placeholder-cancel]')?.addEventListener('click', () => close(state));
  state.modal.querySelector<HTMLSelectElement>('[data-game-placeholder-node]')?.addEventListener('change', event => {
    const next = node(state.game, (event.target as HTMLSelectElement).value);
    if (!next) return;
    state.nodeId = next.id;
    state.sceneId = next.placeholder_scene_asset_id || readyAssets(state.game, 'scene')[0]?.id || '';
    state.placements = (next.placeholder_placements || []).map(normalize);
    state.render();
  });
  state.modal.querySelectorAll<HTMLElement>('[data-game-placeholder-scene]').forEach(button => button.addEventListener('click', () => { state.sceneId = button.dataset.gamePlaceholderScene || ''; state.render(); }));
  state.modal.querySelectorAll<HTMLElement>('[data-game-placeholder-add-role]').forEach(button => button.addEventListener('click', () => { state.placements.push(normalize({ asset_id: button.dataset.gamePlaceholderAddRole || '' }, state.placements.length)); state.render(); }));
  state.modal.querySelectorAll<HTMLElement>('[data-game-placeholder-remove]').forEach(button => button.addEventListener('click', () => { state.placements = state.placements.filter(item => item.id !== button.dataset.gamePlaceholderRemove); state.render(); }));
  state.modal.querySelector('[data-game-placeholder-clear]')?.addEventListener('click', () => { state.placements = []; state.render(); });
  state.modal.querySelectorAll<HTMLInputElement>('[data-game-placeholder-note]').forEach(input => input.addEventListener('change', () => { const placement = state.placements.find(item => item.id === input.dataset.gamePlaceholderNote); if (placement) { placement.note = input.value; placement.pose = input.value; } }));
  state.modal.querySelector('[data-game-placeholder-save]')?.addEventListener('click', async event => { const button = event.currentTarget as HTMLButtonElement; button.disabled = true; button.textContent = '保存中…'; try { await saveDraft(state); state.rt.toast('占位图布局草稿已保存'); } catch (error) { state.rt.toast(`占位图布局保存失败：${error instanceof Error ? error.message : '请稍后重试'}`); } finally { if (state.modal.isConnected) state.render(); } });
  state.modal.querySelector('[data-game-placeholder-generate]')?.addEventListener('click', async () => { try { await saveDraft(state); const response = await fetch(`${state.rt.apiBaseUrl}/games/${state.game.id}/nodes/${state.nodeId}/placeholders/image`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(selectionPayload(state)) }); if (!response.ok) throw new Error(await responseError(response)); const payload = await response.json() as { placeholder?: GameAsset; task?: GameTask }; if (payload.placeholder) state.game.assets = [...(state.game.assets || []).filter(item => item.id !== payload.placeholder?.id), payload.placeholder]; if (payload.task) state.game.tasks = [...(state.game.tasks || []).filter(item => item.id !== payload.task?.id), payload.task]; state.rt.toast('占位图生成任务已创建'); state.render(); void state.refresh(); } catch (error) { state.rt.toast(error instanceof Error ? error.message : '占位图生成失败'); state.render(); } });
  bindDrag(state);
}

/** Open the game node placeholder editor with the same layout controls and history behavior as the drama editor. */
export function openGamePlaceholderModal(game: Game, rt: Runtime, refresh: () => Promise<void>, requestedNodeId?: string) {
  const current = node(game, requestedNodeId) || node(game, game.nodes?.[0]?.id);
  if (!current) { rt.toast('请等待视频节点生成后再配置占位图'); return; }
  activeState?.modal.remove();
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop drama-placeholder-backdrop game-placeholder-backdrop';
  document.body.append(modal);
  const state: PlaceholderState = { game, rt, refresh, modal, nodeId: current.id, sceneId: current.placeholder_scene_asset_id || readyAssets(game, 'scene')[0]?.id || '', placements: (current.placeholder_placements || []).map(normalize), render: () => render(state) };
  activeState = state;
  state.render();
  modal.addEventListener('click', event => { if (event.target === modal) close(state); });
}

/** Refresh an open detached editor after the game task poll has loaded newer node, asset, and task state. */
export function syncGamePlaceholderUi(game: Game) {
  if (!activeState || !activeState.modal.isConnected || activeState.game.id !== game.id) return;
  activeState.game = game;
  if (!selectedNode(activeState)) return close(activeState);
  activeState.render();
}
