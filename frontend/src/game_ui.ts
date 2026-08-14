/** Interactive-game list, editor, graph, and playback UI. */

import type { ApiGame, Game, GameEdge, GameNode, Locale, ModelKind, ModelSettings, VoicePreset } from './models.js';
import { bindGameMaterialInteractions, gameMaterialRailMarkup, selectGameNode } from './game_materials_ui.js';
import { syncGameCoverUi } from './game_cover_ui.js';
import { syncGamePlaceholderUi } from './game_placeholder_ui.js';
import { bindGameGraphCanvas, gameGraphCanvasMarkup } from './game_graph_canvas.js';
import { bindGameCanvasResize } from './game_canvas_resize.js';
import { restoreGameEditorScroll } from './game_scroll_restore.js';
import { openGameScreenplayModal } from './game_screenplay_modal.js';
import { gameGenerationBannerMarkup } from './game_generation_banner_ui.js';
import { syncGameTaskPollingUi } from './game_task_polling_ui.js';
import { gameHasRunningTasks, gameTaskRefreshInterval } from './game_task_refresh_state.js';
import { syncGameVideoBatchGeneration, refreshGameVideoBatchGeneration } from './game_video_batch_generation_ui.js';
import { syncGameBatchVideoCancellation } from './game_video_batch_cancellation_ui.js';
import { confirmAction } from './confirmation_modal.js';
import { icon } from './ui_icons.js';
import { notifyModelTaskFailures, suppressExistingModelTaskFailureNotifications } from './model_task_failure_toast.js';
import { bindGamePlayer, gamePlayerMarkup, type GamePlayerSession } from './game_player_ui.js';

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
  getVoicePresets: () => VoicePreset[];
  loadVoicePresets: () => Promise<void>;
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
  const status = `<span class="status ${game.status === '生成中' ? 'running' : ''}">${game.status === '生成中' ? '◌ ' : ''}${rt().escapeHtml(game.status)}</span>`;
  const retry = game.status === '生成失败'
    ? `<button type="button" class="project-restart-button" data-retry-game="${game.id}">${icon('refresh')}重试</button>`
    : '';
  const statusMarkup = retry ? `<div class="project-status-row">${status}${retry}</div>` : status;
  return `<article class="project-card game-card" data-game="${game.id}"><div class="card-top"><h2>${rt().escapeHtml(game.name)}</h2>${statusMarkup}<div class="tags"><span>${rt().escapeHtml(game.platform)}</span><span>${rt().escapeHtml(game.style)}</span><span>${gameEngine(game.platform)}</span></div></div><div class="metrics"><div><strong>${count}</strong><small>${rt().locale() === 'en' ? 'Video nodes' : '视频节点'}</small></div><div><strong>${game.success_ending_count}</strong><small>${rt().locale() === 'en' ? 'Success endings' : '成功结局'}</small></div><div><strong>${game.failure_ending_count}</strong><small>${rt().locale() === 'en' ? 'Failure endings' : '失败结局'}</small></div><div><strong>${game.branch_min}-${game.branch_max}</strong><small>${rt().locale() === 'en' ? 'Choices' : '每节点选项'}</small></div></div><div class="card-foot"><span>${created}</span><button type="button" class="delete-card-button" data-delete-game="${game.id}">删除</button></div></article>`;
}

export function interactiveGamePage() {
  leaveGameEditor();
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
  modal.innerHTML = `<div class="modal game-modal configured-drama-modal configured-game-modal"><button class="close">×</button><div class="modal-head"><div class="eyebrow">INTERACTIVE GAME / NEW</div><h2>${rt().ui('newGame')}</h2><p>创建后先扩写剧本，再异步拆解为可汇聚的多分支视频节点与选择边。</p></div><label>${rt().ui('gameName')} <em>*</em><input id="game-name" placeholder="例如：雾城抉择" /></label><label>${rt().ui('gameScript')} <em>*</em><textarea id="game-script" rows="7" placeholder="请将一句话创意或互动剧本粘贴到此处，至少 20 个字..."></textarea><div class="hint">${rt().ui('gameScriptHint')} 创建后会先扩写，再生成包含成功/失败结局的互动视频图谱。</div><div class="form-grid game-create-config"><label>${rt().ui('gamePlatform')}<select id="game-platform"><option>微信小游戏</option><option>手机原生游戏</option><option selected>Steam游戏</option></select></label><label>${rt().ui('gameStyle')}<select id="game-style"><option selected>真人风格</option><option>2D动漫</option><option>3D动漫</option></select></label><label>语言模型<select id="game-language-model" disabled><option>正在读取设置…</option></select></label><label>图像模型<select id="game-multimodal-model" disabled><option>正在读取设置…</option></select></label><label>视频模型<select id="game-video-model" disabled><option>正在读取设置…</option></select></label><label>节点视频分辨率<select id="game-resolution"><option selected>720p</option><option>480p</option></select></label><label><span class="game-label-with-info">是否联网扩写剧本 <span class="game-info-tooltip" tabindex="0" role="img" aria-label="联网扩写会消耗更多 token 与时间，但可获取更时新的叙事灵感">ⓘ<span class="game-info-tooltip-content" role="tooltip">联网扩写会消耗更多 token 与时间，但可获取更时新的叙事灵感。</span></span></span><select id="game-web-search"><option value="false" selected>否</option><option value="true">是</option></select></label><label>${rt().ui('successEndings')}<input id="game-success" type="number" min="1" max="100" value="2" /></label><label>${rt().ui('failureEndings')}<input id="game-failure" type="number" min="1" max="200" value="30" /></label><label>${rt().ui('branchRange')}<span class="range-inputs"><input id="game-branch-min" type="number" min="2" max="4" value="2" /><span>～</span><input id="game-branch-max" type="number" min="2" max="4" value="4" /></span></label><label>${rt().ui('durationRange')}<span class="range-inputs"><input id="game-duration-min" type="number" min="4" max="15" value="5" /><span>～</span><input id="game-duration-max" type="number" min="4" max="15" value="15" /></span></label><label class="game-expansion-range"><span>扩写字数</span><div class="game-expansion-range-inputs"><input id="game-expanded-min-chars" type="number" min="1" max="1000000" step="1000" value="5000" aria-label="扩写字数最小值" /><span>至</span><input id="game-expanded-max-chars" type="number" min="1" max="1000000" step="1000" value="10000" aria-label="扩写字数最大值" /><span>字</span></div></label><label>每个视频节点文字上限<input id="game-node-script-max-chars" type="number" min="1" max="1000000" step="10" value="400" aria-label="每个视频节点文字上限" /></label></div><div class="modal-actions"><button class="ghost close-action">${rt().locale() === 'en' ? 'Cancel' : '取消'}</button><button class="primary" id="create-game" disabled>正在读取模型配置…</button></div></div>`;
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
    const expandedMinChars = Number(value('game-expanded-min-chars'));
    const expandedMaxChars = Number(value('game-expanded-max-chars'));
    const nodeScriptMaxChars = Number(value('game-node-script-max-chars'));
    if (!name || script.length < 20) { rt().toast?.('请填写游戏名称，且剧本文本不少于 20 个字'); return; }
    if (branchMin > branchMax || durationMin > durationMax) { rt().toast?.('请检查区间设置'); return; }
    if (!Number.isInteger(expandedMinChars) || !Number.isInteger(expandedMaxChars) || expandedMinChars < 1 || expandedMaxChars < expandedMinChars) { rt().toast?.('扩写字数最小值必须大于零，且不能大于最大值'); return; }
    if (!Number.isInteger(nodeScriptMaxChars) || nodeScriptMaxChars < 1 || nodeScriptMaxChars > 1000000) { rt().toast?.('每个视频节点文字上限须为 1 至 1000000 的整数'); return; }
    const button = modal.querySelector<HTMLButtonElement>('#create-game')!;
    if (!await modelsReady) { rt().toast?.('模型配置加载失败，请检查设置服务后重试'); return; }
    button.disabled = true;
    button.textContent = '创建中…';
    try {
      const response = await fetch(`${rt().apiBaseUrl}/games`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, script, platform: value('game-platform'), style: value('game-style'), success_ending_count: Number(value('game-success')), failure_ending_count: Number(value('game-failure')), branch_min: branchMin, branch_max: branchMax, node_duration_min: durationMin, node_duration_max: durationMax, language_model: value('game-language-model'), multimodal_model: value('game-multimodal-model'), video_model: value('game-video-model'), resolution: value('game-resolution'), enable_web_search: value('game-web-search') === 'true', expanded_script_min_chars: expandedMinChars, expanded_script_max_chars: expandedMaxChars, node_script_max_chars: nodeScriptMaxChars }) });
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

function gameDetailMarkup(game: Game) {
  const nodes = game.nodes || [];
  const screenplay = '';
  return `<div class="game-detail"><div class="drama-detail-toolbar"><button class="back" id="game-back">← 返回</button><div class="drama-project-field"><input id="game-name-input" value="${rt().escapeHtml(game.name)}" maxlength="120" aria-label="游戏名称" autocomplete="off" /></div><div class="drama-top-actions"><button class="ghost" id="game-script">剧本</button><span class="drama-toolbar-divider" aria-hidden="true"></span><button class="ghost" id="game-global-params">☷ 全局参数</button><button class="ghost" id="game-play">▷ 试玩</button><button class="ghost danger-button" id="game-cancel-all-videos">取消所有视频任务</button><div class="game-video-batch-actions" data-game-video-batch-actions><button class="primary" id="game-generate-all-videos">▣ 生成所有视频</button><button class="primary game-video-batch-toggle" type="button" data-game-video-batch-toggle aria-label="选择视频生成方式" aria-haspopup="true" aria-expanded="false"></button><div class="game-video-batch-menu" data-game-video-batch-menu hidden><button type="button" data-game-generate-videos-serial>串行生成</button><button type="button" data-game-generate-videos-parallel>并行生成</button></div></div><button class="primary" id="game-save">▣ 保存</button></div></div>${gameGenerationBannerMarkup(game, rt().escapeHtml)}${screenplay}<div class="game-editor-layout">${gameMaterialRailMarkup()}<section class="panel game-canvas-panel"><div class="panel-title"><div><h2>分支编辑画布</h2><p>${nodes.length} 个视频节点 · ${game.edges?.length || 0} 条选择边</p></div><span class="status ${game.status === '生成中' ? 'running' : ''}">${rt().escapeHtml(game.status)}</span></div><div class="game-canvas-wrap">${nodes.length ? gameGraphCanvasMarkup(game, rt().escapeHtml) : '<div class="game-generating"><div class="empty-icon">◌</div><p>正在扩写剧本并生成分支图谱，请稍候…</p></div>'}</div></section><div class="game-canvas-resizer" data-game-canvas-resizer role="separator" aria-orientation="vertical" aria-label="拖动调整画布宽度"></div><section class="panel game-inspector" id="game-inspector"><div class="inspector-empty"><div class="empty-icon">⌁</div><h3>选择一个节点或选项</h3><p>点击中央画布中的节点配置视频，点击选项边配置选择文案。</p></div></section></div></div>`;
}
let activeSession: GamePlayerSession | null = null;
let activeGameEditorId: string | null = null;
let activeGame: Game | null = null;
let taskTimer: number | null = null;
let selectedGameNode: { gameId: string; nodeId: string } | null = null;
function clearTaskRefresh() { if (taskTimer !== null) window.clearTimeout(taskTimer); taskTimer = null; } function leaveGameEditor() { activeGameEditorId = null; activeGame = null; selectedGameNode = null; clearTaskRefresh(); }
function taskFor(game: Game, type: string, resourceId?: string) { return [...(game.tasks || [])].reverse().find(task => task.type === type && (resourceId === undefined || task.resource_id === resourceId)); }
function selectGameNodeInEditor(game: Game, nodeId: string) { selectedGameNode = { gameId: game.id, nodeId }; selectGameNode(game, nodeId, rt(), taskFor, () => gameDetail(game.id)); }
function restoreSelectedGameNode(game: Game) { const selected = selectedGameNode; if (selected?.gameId === game.id && game.nodes?.some(node => node.id === selected.nodeId)) selectGameNodeInEditor(game, selected.nodeId); }
function scheduleTaskRefresh(game: Game) {
  if (!gameHasRunningTasks(game)) { clearTaskRefresh(); return; } if (taskTimer !== null) return;
  taskTimer = window.setTimeout(() => { taskTimer = null; if (activeGame === game && activeGameEditorId === game.id && rt().active() === 'interactiveGame') void refreshGameTaskState(game); }, gameTaskRefreshInterval(game));
}

async function refreshGameTaskState(game: Game) {
  try {
    const response = await fetch(`${rt().apiBaseUrl}/games/${game.id}`);
    if (!response.ok) throw new Error(await responseError(response));
    const latest = gameFromApi(await response.json() as ApiGame);
    if (activeGame !== game || activeGameEditorId !== game.id || rt().active() !== 'interactiveGame') return;
    notifyModelTaskFailures(latest.tasks || [], message => rt().toast?.(message));
    const graphChanged = syncGameTaskPollingUi({ current: game, latest, runtime: rt(), findTask: taskFor, refresh: () => gameDetail(game.id), onRetryGeneration: id => void retryInteractiveGameGeneration(id, true) });
    refreshGameVideoBatchGeneration(game);
    if (graphChanged) await gameDetail(game.id, game);
  } catch (error) { console.warn('互动游戏任务状态加载失败', error); } finally {
    if (activeGame === game && activeGameEditorId === game.id && rt().active() === 'interactiveGame') scheduleTaskRefresh(game);
  }
}
export async function gameDetail(id: string, initial?: Game, retry = 0, reportTaskFailures = true) {
  if (rt().active() !== 'interactiveGame') return;
  const openingGameEditor = activeGameEditorId !== id;
  activeGameEditorId = id;
  const main = document.querySelector('main');
  if (!main) return;
  let game = initial;
  try { const response = await fetch(`${rt().apiBaseUrl}/games/${id}`); if (response.ok) game = gameFromApi(await response.json() as ApiGame); } catch (error) { if (!game) { rt().toast?.('游戏详情加载失败'); console.error(error); return; } }
  if (!game || activeGameEditorId !== id || rt().active() !== 'interactiveGame') return;
  clearTaskRefresh();
  activeGame = game;
  if (openingGameEditor) suppressExistingModelTaskFailureNotifications(game.tasks || []);
  else if (reportTaskFailures) notifyModelTaskFailures(game.tasks || [], message => rt().toast?.(message));
  const scrollTop = main.scrollTop;
  const inspectorScrollTop = main.querySelector<HTMLElement>('#game-inspector')?.scrollTop;
  main.innerHTML = gameDetailMarkup(game);
  const toolbar = main.querySelector<HTMLElement>('.game-detail .drama-detail-toolbar');
  if (toolbar) toolbar.dataset.gameToolbar = 'true';
  const removeDramaActions = () => main.querySelectorAll('[data-drama-top-actionbar],#drama-open-video-public-prompt').forEach(action => action.remove());
  queueMicrotask(removeDramaActions);
  bindGameEditor(game);
  restoreSelectedGameNode(game);
  syncGameCoverUi(game);
  syncGamePlaceholderUi(game);
  const restoreScrollPositions = () => {
    restoreGameEditorScroll(scrollTop, main);
    if (inspectorScrollTop !== undefined) restoreGameEditorScroll(inspectorScrollTop, main.querySelector<HTMLElement>('#game-inspector'));
  };
  restoreScrollPositions();
  requestAnimationFrame(() => {
    if (activeGameEditorId === id && rt().active() === 'interactiveGame') restoreScrollPositions();
  });
  scheduleTaskRefresh(game);
  if (!game.nodes?.length && retry < 6) window.setTimeout(() => { if (activeGameEditorId === id && rt().active() === 'interactiveGame') void gameDetail(id, game, retry + 1, reportTaskFailures); }, 1000);
}

function openGameGlobalParametersModal(game: Game) {
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop';
  modal.innerHTML = `<div class="modal video-prompt-modal drama-global-params-modal"><button class="close" aria-label="关闭">×</button><div class="modal-head"><h2>全局参数</h2><p>调整后续游戏内容生成的视觉风格和模型配置；不会自动重新生成现有节点。</p></div><div class="drama-global-params-form"><label>视觉风格<select id="game-global-style"><option ${game.style === '真人风格' ? 'selected' : ''}>真人风格</option><option ${game.style === '2D动漫' ? 'selected' : ''}>2D动漫</option><option ${game.style === '3D动漫' ? 'selected' : ''}>3D动漫</option></select></label><label>语言模型<select id="game-model-language"></select></label><label>图像模型<select id="game-model-multimodal"></select></label><label>视频模型<select id="game-model-video"></select></label><label><span class="game-label-with-info">是否联网扩写剧本 <span class="game-info-tooltip" tabindex="0" role="img" aria-label="联网扩写会消耗更多 token 与时间，但可获取更时新的叙事灵感">ⓘ<span class="game-info-tooltip-content" role="tooltip">联网扩写会消耗更多 token 与时间，但可获取更时新的叙事灵感。</span></span></span><select id="game-global-web-search"><option value="false" ${!game.enable_web_search ? 'selected' : ''}>否</option><option value="true" ${game.enable_web_search ? 'selected' : ''}>是</option></select></label></div><div class="video-prompt-actions"><button class="ghost" id="game-global-params-cancel">取消</button><button class="primary" id="save-game-parameters">保存</button></div></div>`;
  document.body.append(modal);
  rt().applyModelSelect(modal, '#game-model-language', 'language', game.language_model);
  rt().applyModelSelect(modal, '#game-model-multimodal', 'multimodal', game.multimodal_model);
  rt().applyModelSelect(modal, '#game-model-video', 'video', game.video_model);
  void rt().loadModelSettings();
  const close = () => modal.remove();
  modal.querySelectorAll('.close,#game-global-params-cancel').forEach(item => item.addEventListener('click', close));
  modal.querySelector('#save-game-parameters')?.addEventListener('click', async () => {
    const value = (id: string) => (modal.querySelector(`#${id}`) as HTMLSelectElement).value;
    const button = modal.querySelector<HTMLButtonElement>('#save-game-parameters')!;
    button.disabled = true;
    button.textContent = '保存中…';
    try {
      const response = await fetch(`${rt().apiBaseUrl}/games/${game.id}/parameters`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ style: value('game-global-style'), language_model: value('game-model-language'), multimodal_model: value('game-model-multimodal'), video_model: value('game-model-video'), enable_web_search: value('game-global-web-search') === 'true' }) });
      if (!response.ok) throw new Error(await responseError(response));
      close();
      rt().toast?.('全局参数和模型配置已保存');
      await gameDetail(game.id);
    } catch (error) { button.disabled = false; button.textContent = '保存'; rt().toast?.(`全局参数保存失败：${error instanceof Error ? error.message : '请稍后重试'}`); console.error(error); }
  });
}

function openEdgeForm(game: Game, sourceNodeId = '', targetNodeId = '') {
  const inspector = document.querySelector<HTMLElement>('#game-inspector');
  const nodes = game.nodes || [];
  if (!inspector || nodes.length < 2) { rt().toast?.('至少需要两个视频节点才能新增选项'); return; }
  const source = nodes.some(node => node.id === sourceNodeId) ? sourceNodeId : nodes[0].id;
  const target = nodes.some(node => node.id === targetNodeId) ? targetNodeId : nodes.find(node => node.id !== source)?.id || source;
  inspector.innerHTML = `<h2>新增选项</h2><label>起始节点<select id="new-edge-source">${nodes.map(node => `<option value="${node.id}" ${node.id === source ? 'selected' : ''}>${rt().escapeHtml(node.title)}</option>`).join('')}</select></label><label>目标节点<select id="new-edge-target">${nodes.map(node => `<option value="${node.id}" ${node.id === target ? 'selected' : ''}>${rt().escapeHtml(node.title)}</option>`).join('')}</select></label><label>选项文案<input id="new-edge-option" placeholder="例如：接受邀请，进入旧城区" /></label><label>排序<input id="new-edge-order" type="number" min="1" value="1" /></label><div class="inspector-actions"><button class="ghost" id="edge-cancel">取消</button><button class="primary" id="edge-create">新增选项</button></div>`;
  inspector.querySelector('#edge-cancel')?.addEventListener('click', () => { inspector.innerHTML = '<div class="inspector-empty"><div class="empty-icon">⌁</div><h3>选择一个节点或选项</h3><p>点击中央画布中的节点配置视频，点击选项边配置选择文案。</p></div>'; });
  inspector.querySelector('#edge-create')?.addEventListener('click', async () => {
    const option = (inspector.querySelector('#new-edge-option') as HTMLInputElement).value.trim();
    if (!option) { rt().toast?.('请填写选项文案'); return; }
    const button = inspector.querySelector<HTMLButtonElement>('#edge-create')!;
    button.disabled = true;
    try {
      const response = await fetch(`${rt().apiBaseUrl}/games/${game.id}/edges`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ source_node_id: (inspector.querySelector('#new-edge-source') as HTMLSelectElement).value, target_node_id: (inspector.querySelector('#new-edge-target') as HTMLSelectElement).value, option_text: option, sort_order: Number((inspector.querySelector('#new-edge-order') as HTMLInputElement).value) }) });
      if (!response.ok) throw new Error(await responseError(response));
      rt().toast?.('选项已新增');
      await gameDetail(game.id);
    } catch (error) { button.disabled = false; rt().toast?.(`选项新增失败：${error instanceof Error ? error.message : '请稍后重试'}`); console.error(error); }
  });
}

function selectEdge(game: Game, edgeId: string) {
  const edge = game.edges?.find(item => item.id === edgeId);
  const inspector = document.querySelector<HTMLElement>('#game-inspector');
  if (!edge || !inspector) return;
  inspector.dataset.gameSelected = `edge:${edgeId}`;
  inspector.innerHTML = `<h2>选项配置</h2><label>起始节点<select id="edge-source">${(game.nodes || []).map(node => `<option value="${node.id}" ${node.id === edge.source_node_id ? 'selected' : ''}>${rt().escapeHtml(node.title)}</option>`).join('')}</select></label><label>目标节点<select id="edge-target">${(game.nodes || []).map(node => `<option value="${node.id}" ${node.id === edge.target_node_id ? 'selected' : ''}>${rt().escapeHtml(node.title)}</option>`).join('')}</select></label><label>选项文案<input id="edge-option" value="${rt().escapeHtml(edge.option_text)}" /></label><label>排序<input id="edge-order" type="number" min="1" value="${edge.sort_order}" /></label><div class="inspector-actions"><button class="ghost" id="edge-save">保存修改</button><button class="danger-button" id="edge-delete">删除选项</button></div>`;
  inspector.querySelector('#edge-save')?.addEventListener('click', async () => { const response = await fetch(`${rt().apiBaseUrl}/games/${game.id}/edges/${edge.id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ option_text: (inspector.querySelector('#edge-option') as HTMLInputElement).value, source_node_id: (inspector.querySelector('#edge-source') as HTMLSelectElement).value, target_node_id: (inspector.querySelector('#edge-target') as HTMLSelectElement).value, sort_order: Number((inspector.querySelector('#edge-order') as HTMLInputElement).value) }) }); if (response.ok) { rt().toast?.('选项已保存'); await gameDetail(game.id); } else rt().toast?.(`选项保存失败：${await responseError(response)}`); });
  inspector.querySelector('#edge-delete')?.addEventListener('click', async () => {
    if (!await confirmAction({ title: '删除选项？', description: '确认删除这个选项？此操作无法恢复。', confirmLabel: '删除选项' })) return;
    try {
      const response = await fetch(`${rt().apiBaseUrl}/games/${game.id}/edges/${edge.id}`, { method: 'DELETE' });
      if (!response.ok) throw new Error(await responseError(response));
      rt().toast?.('选项已删除');
      await gameDetail(game.id);
    } catch (error) { rt().toast?.(`选项删除失败：${error instanceof Error ? error.message : '请稍后重试'}`); }
  });
}

function bindGameEditor(game: Game) {
  document.querySelector('#game-back')?.addEventListener('click', () => { leaveGameEditor(); const navigateToGameList = rt().navigateToGameList; if (navigateToGameList) navigateToGameList(); else { rt().render(); void loadInteractiveGames(); } });
  document.querySelector('#game-play')?.addEventListener('click', () => void playGame(game.id));
  document.querySelector('#game-script')?.addEventListener('click', () => openGameScreenplayModal({
    apiBaseUrl: rt().apiBaseUrl, game, escapeHtml: rt().escapeHtml, toast: message => rt().toast?.(message),
    replaceGame: updated => { const index = interactiveGames.findIndex(item => item.id === updated.id); if (index >= 0) interactiveGames.splice(index, 1, updated); },
    refreshGame: updated => gameDetail(game.id, updated),
  }));
  document.querySelector('#game-global-params')?.addEventListener('click', () => openGameGlobalParametersModal(game));
  document.querySelector('#game-retry-generation')?.addEventListener('click', () => void retryInteractiveGameGeneration(game.id, true));
  const saveGame = async () => {
    const input = document.querySelector<HTMLInputElement>('#game-name-input');
    const name = input?.value.trim() || '';
    if (!name) { rt().toast?.('游戏名称不能为空'); input?.focus(); return; }
    const button = document.querySelector<HTMLButtonElement>('#game-save');
    if (button) { button.disabled = true; button.textContent = '保存中…'; }
    let updated: Game;
    try {
      const response = await fetch(`${rt().apiBaseUrl}/games/${game.id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name }) });
      if (!response.ok) throw new Error(await responseError(response));
      updated = gameFromApi(await response.json() as ApiGame);
      const index = interactiveGames.findIndex(item => item.id === game.id);
      if (index >= 0) interactiveGames.splice(index, 1, updated);
      rt().toast?.('游戏骨架已保存');
    } catch (error) {
      if (button) { button.disabled = false; button.textContent = '▣ 保存'; }
      rt().toast?.(`游戏保存失败：${error instanceof Error ? error.message : '请稍后重试'}`);
      console.error(error);
      return;
    }
    await gameDetail(updated.id, updated, 0, false);
  };
  document.querySelector('#game-save')?.addEventListener('click', () => void saveGame());
  document.querySelector<HTMLInputElement>('#game-name-input')?.addEventListener('keydown', event => { if (event.key === 'Enter') { event.preventDefault(); void saveGame(); } });
  bindGameMaterialInteractions(game, rt(), taskFor, () => gameDetail(game.id), false);
  syncGameVideoBatchGeneration({ apiBaseUrl: rt().apiBaseUrl, game, reloadGame: gameDetail, resolveMediaUrl: rt().resolveMediaUrl, setGenerationButtonLoading: rt().setGenerationButtonLoading, toast: rt().toast });
  syncGameBatchVideoCancellation({ apiBaseUrl: rt().apiBaseUrl, game, reloadGame: gameDetail, toast: rt().toast });
  bindGameGraphCanvas({ game, apiBaseUrl: rt().apiBaseUrl, escapeHtml: rt().escapeHtml, toast: rt().toast, selectNode: nodeId => selectGameNodeInEditor(game, nodeId), selectEdge: edgeId => { selectedGameNode = null; selectEdge(game, edgeId); }, createEdge: (sourceNodeId, targetNodeId) => { selectedGameNode = null; openEdgeForm(game, sourceNodeId, targetNodeId); }, reload: () => gameDetail(game.id) });
  bindGameCanvasResize();
}

async function playGame(gameId: string) {
  try { const game = gameFromApi(await (await fetch(`${rt().apiBaseUrl}/games/${gameId}`)).json() as ApiGame); const response = await fetch(`${rt().apiBaseUrl}/games/${gameId}/sessions`, { method: 'POST' }); if (!response.ok) throw new Error(`HTTP ${response.status}`); activeSession = await response.json() as GamePlayerSession; renderPlayer(game); } catch (error) { rt().toast?.('游戏图谱还没有准备好，请等待生成完成'); console.error(error); }
}

function renderPlayer(game: Game) {
  const main = document.querySelector('main');
  const session = activeSession;
  if (!main || !session) return;
  const node = session.current_node;
  const video = rt().resolveMediaUrl(node.video_url || node.video_history?.at(-1)?.url);
  main.innerHTML = gamePlayerMarkup({ game, session, video, escapeHtml: rt().escapeHtml });
  const restart = () => { activeSession = null; void playGame(game.id); };
  const player = main.querySelector<HTMLElement>('.game-player-page');
  if (!player) return;
  bindGamePlayer(player, {
    back: () => { activeSession = null; void gameDetail(game.id); },
    restart,
    choose: async edgeId => {
      if (!activeSession) return;
      const response = await fetch(`${rt().apiBaseUrl}/games/${game.id}/sessions/${activeSession.id}/choices`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ edge_id: edgeId }) });
      if (!response.ok) { rt().toast?.(`选择失败：${await responseError(response)}`); return; }
      activeSession = await response.json() as GamePlayerSession;
      renderPlayer(game);
    },
  });
}

export async function deleteInteractiveGame(gameId: string, fromDetail = false) {
  const confirmed = await confirmAction({
    title: '删除互动游戏？',
    description: '删除后，分支节点、素材、任务、会话和历史视频都会被永久删除，且无法恢复。',
    confirmLabel: '删除游戏',
  });
  if (!confirmed) return;
  try {
    const response = await fetch(`${rt().apiBaseUrl}/games/${gameId}`, { method: 'DELETE' });
    if (!response.ok) throw new Error(await responseError(response));
    const index = interactiveGames.findIndex(game => game.id === gameId);
    if (index >= 0) interactiveGames.splice(index, 1);
    rt().toast?.('互动游戏及其全部资源已删除');
    const navigateToGameList = rt().navigateToGameList;
    if (fromDetail && navigateToGameList) navigateToGameList();
    else if (rt().active() === 'interactiveGame') rt().render();
    void loadInteractiveGames();
  } catch (error) {
    rt().toast?.(`互动游戏删除失败：${error instanceof Error ? error.message : '请稍后重试'}`);
    console.error(error);
  }
}

/** Requeue a failed screenplay or graph task from its persisted checkpoint for either list-card or workbench retry actions. */
export async function retryInteractiveGameGeneration(gameId: string, fromDetail = false) {
  try {
    const response = await fetch(`${rt().apiBaseUrl}/games/${gameId}/script-decomposition/retry`, { method: 'POST' });
    if (!response.ok) throw new Error(await responseError(response));
    rt().toast?.('已从上次保存的进度继续生成');
    if (fromDetail) await gameDetail(gameId);
    else await loadInteractiveGames();
  } catch (error) { rt().toast?.(`重试失败：${error instanceof Error ? error.message : '请稍后重试'}`); console.error(error); }
}
