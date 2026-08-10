/** Interactive-game list, editor, graph, and playback UI. */

import type { ApiGame, Game, GameEdge, GameNode, Locale, ModelKind, ModelSettings } from './models.js';
import { icon } from './ui_icons.js';
import { notifyModelTaskFailures } from './model_task_failure_toast.js';

type GameSession = { id: string; game_id: string; current_node_id: string; status: string; path: { edge_id: string; option_text: string }[]; current_node: GameNode; choices: GameEdge[] };
type GameRuntime = {
  apiBaseUrl: string;
  locale: () => Locale;
  active: () => string;
  ui: (key: string) => string;
  escapeHtml: (value: unknown) => string;
  resolveMediaUrl: (value?: string | null) => string;
  applyModelSelect: (root: HTMLElement, selector: string, kind: ModelKind, selected?: string) => void;
  loadModelSettings: () => Promise<boolean>;
  render: () => void;
  toast: (message: string) => void;
  deleteInteractiveGame: (gameId: string, fromDetail?: boolean) => Promise<void>;
  setGenerationButtonLoading: (button: HTMLButtonElement, loading: boolean, idleText: string) => void;
  navigateToGameDetail?: (id: string) => void;
  navigateToGameList?: () => void;
};

let runtime: GameRuntime;
const rt = () => runtime;
export const interactiveGames: Game[] = [];
export function configureGameRuntime(value: GameRuntime) { runtime = value; }

function gameEngine(platform: string) { return platform === 'Steam游戏' ? 'Unity' : 'Cocos Creator'; }
async function responseError(response: Response) { const body = await response.json().catch(() => null) as { detail?: unknown; message?: unknown } | null; return typeof body?.detail === 'string' ? body.detail : typeof body?.message === 'string' ? body.message : `HTTP ${response.status}`; }
function gameFromApi(game: ApiGame): Game { return { ...game, nodes: game.nodes || [], edges: game.edges || [], assets: game.assets || [] }; }
function gameCard(game: Game) {
  const count = game.node_count ?? game.nodes?.length ?? 0;
  const created = game.created_at?.slice(0, 16).replace('T', ' ') || '刚刚';
  return `<article class="project-card game-card" data-game="${game.id}"><div class="card-top"><h2>${rt().escapeHtml(game.name)}</h2><span class="status ${game.status === '生成中' ? 'running' : ''}">${game.status === '生成中' ? '◌ ' : ''}${rt().escapeHtml(game.status)}</span><div class="tags"><span>${rt().escapeHtml(game.platform)}</span><span>${rt().escapeHtml(game.style)}</span><span>${gameEngine(game.platform)}</span></div></div><div class="metrics"><div><strong>${count}</strong><small>${rt().locale() === 'en' ? 'Video nodes' : '视频节点'}</small></div><div><strong>${game.success_ending_count}</strong><small>${rt().locale() === 'en' ? 'Success endings' : '成功结局'}</small></div><div><strong>${game.failure_ending_count}</strong><small>${rt().locale() === 'en' ? 'Failure endings' : '失败结局'}</small></div><div><strong>${game.branch_min}-${game.branch_max}</strong><small>${rt().locale() === 'en' ? 'Choices' : '每节点选项'}</small></div></div><div class="card-foot"><span>${created}</span><button type="button" class="delete-card-button" data-delete-game="${game.id}">删除</button></div></article>`;
}

export function interactiveGamePage() {
  return `<header><div><div class="eyebrow">${rt().ui('workspace')}</div><h1>${rt().ui('interactiveGameTitle')}</h1><p>${rt().ui('gameEditorDescription')}</p></div><div class="header-actions"><a class="ghost game-demo-link" href="/interactive-game-demo/index.html" target="_blank" rel="noreferrer">${rt().ui('playOfflineDemo')}</a><button class="primary" id="new-game">${rt().ui('newGame')}</button></div></header><section class="toolbar"><div class="search">⌕ <input placeholder="${rt().ui('gameSearch')}" /></div><button class="ghost" id="refresh-games">${rt().ui('refresh')}</button><span class="toolbar-count">${interactiveGames.length} ${rt().ui('gameProjects')}</span></section>${interactiveGames.length ? `<section class="cards">${interactiveGames.map(gameCard).join('')}</section>` : `<div class="empty game-empty"><div class="empty-icon">◉</div><h2>${rt().ui('interactiveGameTitle')}</h2><p>${rt().ui('noGames')}</p><button class="primary" id="new-game-empty">${rt().ui('newGame')}</button></div>`}`;
}

export async function loadInteractiveGames() {
  try {
    const response = await fetch(`${rt().apiBaseUrl}/games`);
    if (!response.ok) return;
    interactiveGames.splice(0, interactiveGames.length, ...(await response.json() as ApiGame[]).map(gameFromApi));
    if (rt().active() === 'interactiveGame') rt().render();
  } catch (error) { console.warn('互动游戏列表加载失败', error); }
}

export function openGameModal() {
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop';
  modal.innerHTML = `<div class="modal game-modal"><button class="close">×</button><div class="modal-head"><div class="eyebrow">INTERACTIVE GAME / NEW</div><h2>${rt().ui('newGame')}</h2><p>${rt().ui('gameEditorDescription')}</p></div><label>${rt().ui('gameName')} <em>*</em><input id="game-name" placeholder="例如：雾城抉择" /></label><label>${rt().ui('gameScript')} <em>*</em><textarea id="game-script" rows="7" placeholder="请将互动剧本粘贴到此处，至少 20 个字..."></textarea><div class="hint">${rt().ui('gameScriptHint')}</div><div class="form-grid"><label>${rt().ui('gamePlatform')}<select id="game-platform"><option>微信小游戏</option><option>手机原生游戏</option><option selected>Steam游戏</option></select></label><label>${rt().ui('gameStyle')}<select id="game-style"><option selected>真人风格</option><option>2D动漫</option><option>3D动漫</option></select></label><label>语言模型<select id="game-language-model" disabled><option>正在读取设置…</option></select></label><label>图像模型<select id="game-multimodal-model" disabled><option>正在读取设置…</option></select></label><label>视频模型<select id="game-video-model" disabled><option>正在读取设置…</option></select></label><label>${rt().ui('successEndings')}<input id="game-success" type="number" min="1" value="2" /></label><label>${rt().ui('failureEndings')}<input id="game-failure" type="number" min="1" value="30" /></label><label>${rt().ui('branchRange')}<span class="range-inputs"><input id="game-branch-min" type="number" min="2" max="4" value="2" /><span>～</span><input id="game-branch-max" type="number" min="2" max="4" value="4" /></span></label><label>${rt().ui('durationRange')}<span class="range-inputs"><input id="game-duration-min" type="number" min="1" value="5" /><span>～</span><input id="game-duration-max" type="number" min="1" value="30" /></span></label></div><div class="modal-actions"><button class="ghost close-action">${rt().locale() === 'en' ? 'Cancel' : '取消'}</button><button class="primary" id="create-game" disabled>正在读取模型配置…</button></div></div>`;
  document.body.append(modal);
  const createButton = modal.querySelector<HTMLButtonElement>('#create-game')!;
  const modelsReady = rt().loadModelSettings().then(loaded => {
    if (!modal.isConnected) return loaded;
    rt().applyModelSelect(modal, '#game-language-model', 'language');
    rt().applyModelSelect(modal, '#game-multimodal-model', 'multimodal');
    rt().applyModelSelect(modal, '#game-video-model', 'video');
    modal.querySelectorAll<HTMLSelectElement>('#game-language-model,#game-multimodal-model,#game-video-model').forEach(select => { select.disabled = !loaded; });
    createButton.disabled = !loaded;
    createButton.textContent = loaded ? rt().ui('createGame') : '模型配置加载失败';
    return loaded;
  });
  const close = () => modal.remove();
  modal.querySelectorAll('.close,.close-action').forEach(item => item.addEventListener('click', close));
  modal.querySelector('#create-game')?.addEventListener('click', async () => {
    const value = (id: string) => (modal.querySelector(`#${id}`) as HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement).value;
    const name = value('game-name').trim();
    const script = value('game-script').trim();
    const branchMin = Number(value('game-branch-min'));
    const branchMax = Number(value('game-branch-max'));
    const durationMin = Number(value('game-duration-min'));
    const durationMax = Number(value('game-duration-max'));
    if (!name || script.length < 20) { rt().toast?.('请填写游戏名称，且剧本文本不少于 20 个字'); return; }
    if (branchMin > branchMax || durationMin > durationMax) { rt().toast?.('请检查区间设置'); return; }
    const button = modal.querySelector<HTMLButtonElement>('#create-game')!;
    if (!await modelsReady) { rt().toast?.('模型配置加载失败，请检查设置服务后重试'); return; }
    button.disabled = true;
    button.textContent = '创建中…';
    try {
      const response = await fetch(`${rt().apiBaseUrl}/games`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, script, platform: value('game-platform'), style: value('game-style'), success_ending_count: Number(value('game-success')), failure_ending_count: Number(value('game-failure')), branch_min: branchMin, branch_max: branchMax, node_duration_min: durationMin, node_duration_max: durationMax, language_model: value('game-language-model'), multimodal_model: value('game-multimodal-model'), video_model: value('game-video-model') }) });
      if (!response.ok) throw new Error(await responseError(response));
      const game = gameFromApi(await response.json() as ApiGame);
      interactiveGames.unshift(game);
      close();
      const navigateToGameDetail = rt().navigateToGameDetail;
      if (navigateToGameDetail) navigateToGameDetail(game.id);
      else await gameDetail(game.id, game);
    } catch (error) { button.disabled = false; button.textContent = rt().ui('createGame'); rt().toast?.(`创建失败：${error instanceof Error ? error.message : '请稍后重试'}`); console.error(error); }
  });
}

function graphMarkup(game: Game) {
  const nodes = game.nodes || [];
  const edges = game.edges || [];
  const map = new Map(nodes.map(node => [node.id, node]));
  const width = Math.max(1200, ...nodes.map(node => node.position_x + 240));
  const height = Math.max(560, ...nodes.map(node => node.position_y + 160));
  const edgeMarkup = edges.map(edge => { const source = map.get(edge.source_node_id); const target = map.get(edge.target_node_id); if (!source || !target) return ''; const x1 = source.position_x + 180; const y1 = source.position_y + 45; const x2 = target.position_x; const y2 = target.position_y + 45; return `<line class="game-edge-line" data-game-edge="${edge.id}" x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}"></line><button class="game-edge-label" data-game-edge="${edge.id}" style="left:${(x1 + x2) / 2}px;top:${(y1 + y2) / 2}px">${rt().escapeHtml(edge.option_text)}</button>`; }).join('');
  return `<div class="game-graph" style="width:${width}px;height:${height}px"><svg class="game-edges" width="${width}" height="${height}">${edgeMarkup.replace(/<button[\s\S]*?<\/button>/g, '')}</svg>${edgeMarkup.match(/<button[\s\S]*?<\/button>/g)?.join('') || ''}${nodes.map(node => `<button class="game-node ${node.node_type}" data-game-node="${node.id}" style="left:${node.position_x}px;top:${node.position_y}px"><span>${node.node_type === 'start' ? '起点' : node.node_type === 'success' ? '成功' : node.node_type === 'failure' ? '失败' : '节点'}</span><strong>${rt().escapeHtml(node.title)}</strong><small>${node.duration_seconds}s · ${rt().escapeHtml(node.status)}</small></button>`).join('')}</div>`;
}

function gameDetailMarkup(game: Game) {
  const nodes = game.nodes || [];
  const assets = game.assets || [];
  const assetRows = assets.map(asset => `<div class="asset-row"><div class="asset-thumb ${asset.type === 'scene' ? 'green' : ''}">${asset.type === 'character' ? icon('character') : asset.type === 'scene' ? '✦' : '◆'}</div><div><b>${rt().escapeHtml(asset.name)}</b><span>${rt().escapeHtml(asset.type)} · ${rt().escapeHtml(asset.status)}</span><p>${rt().escapeHtml(asset.prompt)}</p></div></div>`).join('');
  return `<div class="game-detail"><button class="back" id="game-back">← 返回游戏列表</button><header><div><div class="eyebrow">INTERACTIVE GAME / ${rt().escapeHtml(game.name)}</div><h1>${rt().escapeHtml(game.name)}</h1><p>${rt().ui('gameEditorDescription')}</p><div class="game-meta"><span>${rt().escapeHtml(game.platform)}</span><span>${rt().escapeHtml(game.style)}</span><span>${gameEngine(game.platform)}</span><span>语言：${rt().escapeHtml(game.language_model)}</span><span>图像：${rt().escapeHtml(game.multimodal_model)}</span><span>视频：${rt().escapeHtml(game.video_model)}</span></div></div><div class="header-actions"><button class="ghost" id="game-models">模型配置</button><button class="primary" id="game-play">试玩游戏</button><button class="ghost" id="game-refresh">刷新图谱</button><button class="ghost danger-button" id="game-delete">删除游戏</button><button class="primary" id="game-add-edge">新增选项</button></div></header><div class="game-editor-layout"><section class="panel game-assets-panel"><div class="panel-title"><h2>基础组成元素</h2><span>${assets.length} 个元素</span></div>${assetRows}</section><section class="panel game-canvas-panel"><div class="panel-title"><div><h2>分支编辑画布</h2><p>${nodes.length} 个视频节点 · ${game.edges?.length || 0} 条选择边</p></div><span class="status ${game.status === '生成中' ? 'running' : ''}">${rt().escapeHtml(game.status)}</span></div><div class="game-canvas-wrap">${nodes.length ? graphMarkup(game) : '<div class="game-generating"><div class="empty-icon">◌</div><p>分支图谱生成中，请稍候…</p></div>'}</div></section><section class="panel game-inspector" id="game-inspector"><div class="inspector-empty"><div class="empty-icon">⌁</div><h3>选择一个节点或选项</h3><p>点击中央画布中的节点配置视频，点击选项边配置选择文案。</p></div></section></div></div>`;
}

let activeSession: GameSession | null = null;
let activeNodeId: string | null = null;
let taskTimer: number | null = null;
function taskFor(game: Game, type: string, resourceId?: string) { return [...(game.tasks || [])].reverse().find(task => task.type === type && (resourceId === undefined || task.resource_id === resourceId)); }
function scheduleTaskRefresh(game: Game) { if (!(game.tasks || []).some(task => task.status === '生成中')) { if (taskTimer !== null) window.clearTimeout(taskTimer); taskTimer = null; return; } if (taskTimer === null) taskTimer = window.setTimeout(() => { taskTimer = null; void gameDetail(game.id); }, 1000); }

export async function gameDetail(id: string, initial?: Game, retry = 0) {
  if (rt().active() !== 'interactiveGame') return;
  const main = document.querySelector('main');
  if (!main) return;
  let game = initial;
  try { const response = await fetch(`${rt().apiBaseUrl}/games/${id}`); if (response.ok) game = gameFromApi(await response.json() as ApiGame); } catch (error) { if (!game) { rt().toast?.('游戏详情加载失败'); console.error(error); return; } }
  if (!game) return;
  notifyModelTaskFailures(game.tasks || [], message => rt().toast?.(message));
  main.innerHTML = gameDetailMarkup(game);
  bindGameEditor(game);
  scheduleTaskRefresh(game);
  if (!game.nodes?.length && retry < 6) window.setTimeout(() => void gameDetail(id, game, retry + 1), 1000);
}

function selectNode(game: Game, nodeId: string) {
  const node = game.nodes?.find(item => item.id === nodeId);
  const inspector = document.querySelector<HTMLElement>('#game-inspector');
  if (!node || !inspector) return;
  activeNodeId = nodeId;
  const task = taskFor(game, 'node_video_generation', nodeId);
  inspector.innerHTML = `<div class="inspector-head"><h2>${rt().escapeHtml(node.title)}</h2><span class="status ${node.status === '生成中' ? 'running' : ''}">${rt().escapeHtml(node.status)}</span></div><label>节点标题<input id="node-title" value="${rt().escapeHtml(node.title)}" /></label><label>原始文本<textarea id="node-original" rows="4">${rt().escapeHtml(node.original_text)}</textarea></label><label>视频 Prompt<textarea id="node-prompt" rows="7">${rt().escapeHtml(node.prompt)}</textarea></label><label>视频时长（秒）<input id="node-duration" type="number" min="1" value="${node.duration_seconds}" /></label><div class="inspector-actions"><button class="ghost" id="node-save">保存修改</button><button class="primary" id="node-generate">生成节点视频</button></div><div class="history-list"><h3>视频历史</h3>${(node.video_history || []).map(video => `<div>${rt().escapeHtml(video.generated_at || '')} · ${rt().escapeHtml(video.url || '等待生成')}</div>`).join('') || '<p>暂无历史视频</p>'}</div>`;
  const generate = inspector.querySelector<HTMLButtonElement>('#node-generate');
  if (generate) { rt().setGenerationButtonLoading(generate, Boolean(task?.status === '生成中' || node.status === '生成中'), '生成节点视频'); if (task?.status === '生成中') generate.textContent = `⟳ 生成节点视频 ${task.progress || 0}%`; }
  inspector.querySelector('#node-save')?.addEventListener('click', async () => { await fetch(`${rt().apiBaseUrl}/games/${game.id}/nodes/${nodeId}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ title: (inspector.querySelector('#node-title') as HTMLInputElement).value, original_text: (inspector.querySelector('#node-original') as HTMLTextAreaElement).value, prompt: (inspector.querySelector('#node-prompt') as HTMLTextAreaElement).value, duration_seconds: Number((inspector.querySelector('#node-duration') as HTMLInputElement).value) }) }); rt().toast?.('节点已保存'); await gameDetail(game.id); });
  inspector.querySelector('#node-generate')?.addEventListener('click', async () => { try { const response = await fetch(`${rt().apiBaseUrl}/games/${game.id}/nodes/${nodeId}/video`, { method: 'POST' }); if (!response.ok) throw new Error(await responseError(response)); rt().toast?.('节点视频任务已创建'); await gameDetail(game.id); } catch (error) { rt().toast?.(`节点视频任务创建失败：${error instanceof Error ? error.message : '请稍后重试'}`); console.error(error); } });
}

function openGameModelSelectionModal(game: Game) {
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop';
  modal.innerHTML = `<div class="modal game-modal"><button class="close">×</button><div class="modal-head"><div class="eyebrow">INTERACTIVE GAME / MODELS</div><h2>修改模型配置</h2><p>为当前互动游戏选择语言、图像和视频模型。</p></div><label>语言模型<select id="game-model-language"></select></label><label>图像模型<select id="game-model-multimodal"></select></label><label>视频模型<select id="game-model-video"></select></label><div class="modal-actions"><button class="ghost close-action">取消</button><button class="primary" id="save-game-models">保存</button></div></div>`;
  document.body.append(modal);
  rt().applyModelSelect(modal, '#game-model-language', 'language', game.language_model);
  rt().applyModelSelect(modal, '#game-model-multimodal', 'multimodal', game.multimodal_model);
  rt().applyModelSelect(modal, '#game-model-video', 'video', game.video_model);
  void rt().loadModelSettings();
  const close = () => modal.remove();
  modal.querySelectorAll('.close,.close-action').forEach(item => item.addEventListener('click', close));
  modal.querySelector('#save-game-models')?.addEventListener('click', async () => {
    const value = (id: string) => (modal.querySelector(`#${id}`) as HTMLSelectElement).value;
    const button = modal.querySelector<HTMLButtonElement>('#save-game-models')!;
    button.disabled = true;
    try {
      const response = await fetch(`${rt().apiBaseUrl}/games/${game.id}/models`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ language_model: value('game-model-language'), multimodal_model: value('game-model-multimodal'), video_model: value('game-model-video') }) });
      if (!response.ok) throw new Error(await responseError(response));
      close();
      rt().toast?.('互动游戏模型配置已保存');
      await gameDetail(game.id);
    } catch (error) { button.disabled = false; rt().toast?.(`模型配置保存失败：${error instanceof Error ? error.message : '请稍后重试'}`); console.error(error); }
  });
}

function openEdgeForm(game: Game) {
  const inspector = document.querySelector<HTMLElement>('#game-inspector');
  const nodes = game.nodes || [];
  if (!inspector || nodes.length < 2) { rt().toast?.('至少需要两个视频节点才能新增选项'); return; }
  inspector.innerHTML = `<h2>新增选项</h2><label>起始节点<select id="new-edge-source">${nodes.map(node => `<option value="${node.id}">${rt().escapeHtml(node.title)}</option>`).join('')}</select></label><label>目标节点<select id="new-edge-target">${nodes.map(node => `<option value="${node.id}">${rt().escapeHtml(node.title)}</option>`).join('')}</select></label><label>选项文案<input id="new-edge-option" placeholder="例如：接受邀请，进入旧城区" /></label><label>排序<input id="new-edge-order" type="number" min="1" value="1" /></label><div class="inspector-actions"><button class="ghost" id="edge-cancel">取消</button><button class="primary" id="edge-create">新增选项</button></div>`;
  inspector.querySelector('#edge-cancel')?.addEventListener('click', () => { inspector.innerHTML = '<div class="inspector-empty"><div class="empty-icon">⌁</div><h3>选择一个节点或选项</h3><p>点击中央画布中的节点配置视频，点击选项边配置选择文案。</p></div>'; });
  inspector.querySelector('#edge-create')?.addEventListener('click', async () => {
    const option = (inspector.querySelector('#new-edge-option') as HTMLInputElement).value.trim();
    if (!option) { rt().toast?.('请填写选项文案'); return; }
    const button = inspector.querySelector<HTMLButtonElement>('#edge-create')!;
    button.disabled = true;
    try {
      const response = await fetch(`${rt().apiBaseUrl}/games/${game.id}/edges`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ source_node_id: (inspector.querySelector('#new-edge-source') as HTMLSelectElement).value, target_node_id: (inspector.querySelector('#new-edge-target') as HTMLSelectElement).value, option_text: option, sort_order: Number((inspector.querySelector('#new-edge-order') as HTMLInputElement).value) }) });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      rt().toast?.('选项已新增');
      await gameDetail(game.id);
    } catch (error) { button.disabled = false; rt().toast?.('选项新增失败'); console.error(error); }
  });
}

function selectEdge(game: Game, edgeId: string) {
  const edge = game.edges?.find(item => item.id === edgeId);
  const inspector = document.querySelector<HTMLElement>('#game-inspector');
  if (!edge || !inspector) return;
  inspector.innerHTML = `<h2>选项配置</h2><label>选项文案<input id="edge-option" value="${rt().escapeHtml(edge.option_text)}" /></label><label>目标节点<select id="edge-target">${(game.nodes || []).map(node => `<option value="${node.id}" ${node.id === edge.target_node_id ? 'selected' : ''}>${rt().escapeHtml(node.title)}</option>`).join('')}</select></label><label>排序<input id="edge-order" type="number" min="1" value="${edge.sort_order}" /></label><div class="inspector-actions"><button class="ghost" id="edge-save">保存修改</button><button class="danger-button" id="edge-delete">删除选项</button></div>`;
  inspector.querySelector('#edge-save')?.addEventListener('click', async () => { const response = await fetch(`${rt().apiBaseUrl}/games/${game.id}/edges/${edge.id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ option_text: (inspector.querySelector('#edge-option') as HTMLInputElement).value, target_node_id: (inspector.querySelector('#edge-target') as HTMLSelectElement).value, sort_order: Number((inspector.querySelector('#edge-order') as HTMLInputElement).value) }) }); if (response.ok) { rt().toast?.('选项已保存'); await gameDetail(game.id); } });
  inspector.querySelector('#edge-delete')?.addEventListener('click', async () => { if (!window.confirm('确认删除这个选项？')) return; await fetch(`${rt().apiBaseUrl}/games/${game.id}/edges/${edge.id}`, { method: 'DELETE' }); rt().toast?.('选项已删除'); await gameDetail(game.id); });
}

function bindGameEditor(game: Game) {
  document.querySelector('#game-back')?.addEventListener('click', () => { const navigateToGameList = rt().navigateToGameList; if (navigateToGameList) navigateToGameList(); else { rt().render(); void loadInteractiveGames(); } });
  document.querySelector('#game-play')?.addEventListener('click', () => void playGame(game.id));
  document.querySelector('#game-refresh')?.addEventListener('click', () => void gameDetail(game.id));
  document.querySelector('#game-delete')?.addEventListener('click', () => void rt().deleteInteractiveGame(game.id, true));
  document.querySelector('#game-models')?.addEventListener('click', () => openGameModelSelectionModal(game));
  document.querySelectorAll<HTMLElement>('[data-game-node]').forEach(item => item.addEventListener('click', () => selectNode(game, item.dataset.gameNode!)));
  document.querySelectorAll<HTMLElement>('[data-game-edge]').forEach(item => item.addEventListener('click', () => selectEdge(game, item.dataset.gameEdge!)));
  document.querySelector('#game-add-edge')?.addEventListener('click', () => openEdgeForm(game));
}

async function playGame(gameId: string) {
  try { const game = gameFromApi(await (await fetch(`${rt().apiBaseUrl}/games/${gameId}`)).json() as ApiGame); const response = await fetch(`${rt().apiBaseUrl}/games/${gameId}/sessions`, { method: 'POST' }); if (!response.ok) throw new Error(`HTTP ${response.status}`); activeSession = await response.json() as GameSession; renderPlayer(game); } catch (error) { rt().toast?.('游戏图谱还没有准备好，请等待生成完成'); console.error(error); }
}

function renderPlayer(game: Game) {
  const main = document.querySelector('main');
  const session = activeSession;
  if (!main || !session) return;
  const node = session.current_node;
  const video = rt().resolveMediaUrl(node.video_url || node.video_history?.at(-1)?.url);
  main.innerHTML = `<div class="game-player-page"><div class="game-player-topbar"><button class="back" id="game-player-back">← 返回编辑器</button><strong>${rt().escapeHtml(game.name)}</strong><button class="ghost" id="game-player-restart">重新开始</button></div><main class="game-player-layout"><section class="game-player-stage"><div class="game-player-video-wrap">${video ? `<video controls autoplay playsinline src="${rt().escapeHtml(video)}"></video>` : `<div class="game-player-video-fallback"><strong>${rt().escapeHtml(node.title)}</strong><p>该节点还没有生成视频。</p></div>`}</div></section><aside class="game-player-choice-panel"><p>当前路径：${rt().escapeHtml(session.path.map(item => item.option_text).join(' → ') || '起点')}</p>${session.status !== 'active' ? '<h2>故事已结束</h2><button class="primary" id="game-player-ending-restart">再玩一次</button>' : `<div class="game-player-choices">${session.choices.map((edge, index) => `<button class="game-player-choice" data-game-player-choice="${edge.id}"><b>${String.fromCharCode(65 + index)}</b><span>${rt().escapeHtml(edge.option_text)}</span></button>`).join('')}</div>`}</aside></main></div>`;
  const restart = () => { activeSession = null; void playGame(game.id); };
  document.querySelector('#game-player-back')?.addEventListener('click', () => { activeSession = null; void gameDetail(game.id); });
  document.querySelector('#game-player-restart,#game-player-ending-restart')?.addEventListener('click', restart);
  document.querySelectorAll<HTMLElement>('[data-game-player-choice]').forEach(button => button.addEventListener('click', async () => { const edgeId = button.dataset.gamePlayerChoice; if (!edgeId || !activeSession) return; const response = await fetch(`${rt().apiBaseUrl}/games/${game.id}/sessions/${activeSession.id}/choices`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ edge_id: edgeId }) }); if (response.ok) { activeSession = await response.json() as GameSession; renderPlayer(game); } }));
}

export async function deleteInteractiveGame(gameId: string, fromDetail = false) {
  if (!window.confirm('删除互动游戏后，分支节点、素材、任务、会话和历史视频都会被永久删除，确定继续吗？')) return;
  const response = await fetch(`${rt().apiBaseUrl}/games/${gameId}`, { method: 'DELETE' });
  if (!response.ok) { rt().toast?.('互动游戏删除失败，请稍后重试'); return; }
  const index = interactiveGames.findIndex(game => game.id === gameId);
  if (index >= 0) interactiveGames.splice(index, 1);
  rt().toast?.('互动游戏及其全部资源已删除');
  const navigateToGameList = rt().navigateToGameList;
  if (fromDetail && navigateToGameList) navigateToGameList();
  else if (rt().active() === 'interactiveGame') rt().render();
  void loadInteractiveGames();
}
