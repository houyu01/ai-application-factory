import './style.css';

import { configureGameRuntime, deleteInteractiveGame, gameDetail as gameDetailCore, interactiveGamePage, interactiveGames, loadInteractiveGames, openGameModal } from './game_ui.js';
import * as dramaCore from './drama_core_ui.js';
import * as dramaModal from './drama_modal_ui.js';
import * as dramaShot from './drama_shot_ui.js';
import { applyDramaTaskUpdate, configureDramaPartialRefresh } from './drama_partial_refresh.js';
import { configureDramaTaskPolling } from './drama_task_polling.js';
import { activeDramaProject, dramaViewState, setActiveDramaProject } from './drama_state.js';
import type { StorageSettingsResponse } from './drama_modal_ui.js';
import type { ApiGame, ApiProject, DramaAsset, DramaAssetImageHistory, DramaAssetKind, DramaAssetMetadata, DramaAssetVariant, DramaEpisode, DramaPlacement, DramaPromptAssetType, DramaPromptNode, DramaShot, DramaShotVersion, Game, GameAsset, GameEdge, GameNode, GameTask, GenerationTask, Locale, ModelKind, ModelSettings, Project, VoicePreset } from './models.js';
import { apiKeyVisibilityIcon, applyModelSelect, configureSettingsRuntime, loadModelSettings, loadVoicePresets, modelChoices, modelEditorValues, modelSettingsCard, refreshModelSelects, renderModelEditor, voiceCatalogCard, voiceOptions, voicePreset } from './settings_ui.js';

const { dramaAssets, dramaAssetDrawer, dramaKindLabel, dramaSelectedShot, dramaShots, openAssetPublicPromptModal, readDramaPromptNodes, renderDramaPromptNodes, resolveMediaUrl, serializeDramaPromptNodes, setGenerationButtonLoading, setupDramaRichPromptEditor, syncDramaShotReferencePanel, loadDramaDetail: loadDramaDetailCore, loadDramaProjects, deleteDramaProject } = dramaCore;
const { bindDramaWorkspace, loadStorageSettings, storageEndpointPlaceholder, storageField, storageProviderStatus } = dramaModal;
const API_BASE_URL = (import.meta.env.VITE_API_BASE_URL || 'http://127.0.0.1:8090/api').replace(/\/$/, '');
const copy = {
  workspace: { zh: '创作空间 / AI 应用平台', en: 'WORKSPACE / AI PLATFORM' },
  dramaTitle: { zh: '短剧创作', en: 'Drama Studio' },
  dramaDescription: { zh: '统一管理每一部短剧的角色、场景、道具、剧集与分镜。', en: 'Manage characters, scenes, props, episodes, and shots in one place.' },
  newDrama: { zh: '＋ 新建短剧', en: '＋ New Drama' },
  search: { zh: '搜索短剧名称、模型或配置', en: 'Search dramas, models, or settings' },
  refresh: { zh: '刷新', en: 'Refresh' },
  projects: { zh: '个项目', en: 'projects' },
  ready: { zh: '配置就绪', en: 'Ready' },
  settingsEyebrow: { zh: '系统 / 模型与服务', en: 'SYSTEM / MODELS & SERVICES' },
  settingsTitle: { zh: '配置', en: 'Settings' },
  settingsDescription: { zh: '配置各类模型的 endpoint、凭证和可选模型列表，项目创建后可单独选择模型。', en: 'Configure endpoints, credentials, and model choices. Each project can select its own models.' },
  interactiveGameDescription: { zh: '把视频节点、选择分支和多种结局组织成可试玩的互动视频游戏。', en: 'Turn video nodes, branching choices, and multiple endings into a playable interactive video game.' },
  playOfflineDemo: { zh: '试玩离线样例', en: 'Play offline sample' },
  interactiveGameTitle: { zh: '互动游戏生成', en: 'Interactive Game' },
  newGame: { zh: '＋ 创建互动游戏', en: '＋ New Interactive Game' },
  gameSearch: { zh: '搜索游戏名称、平台或风格', en: 'Search games, platforms, or styles' },
  gameProjects: { zh: '个游戏', en: 'games' },
  noGames: { zh: '还没有互动游戏，先创建一个分支视频游戏吧。', en: 'No interactive games yet. Create a branching video game to get started.' },
  gameEditorDescription: { zh: '视频节点、选择边和成功/失败结局组成的互动剧图谱。', en: 'An interactive drama graph made of video nodes, choice edges, and endings.' },
  createGame: { zh: '创建并进入编辑器', en: 'Create and open editor' },
  gameName: { zh: '游戏名称', en: 'Game name' },
  gameScript: { zh: '基础游戏剧本', en: 'Game script' },
  gamePlatform: { zh: '发布平台', en: 'Target platform' },
  gameStyle: { zh: '视觉风格', en: 'Visual style' },
  successEndings: { zh: '成功结局数量', en: 'Success endings' },
  failureEndings: { zh: '失败结局数量', en: 'Failure endings' },
  branchRange: { zh: '每条分支数量', en: 'Choices per node' },
  durationRange: { zh: '节点视频时长（秒）', en: 'Node duration (seconds)' },
  gameScriptHint: { zh: '剧本文本不少于 20 个字，创建后会异步拆解为视频节点和选择边。', en: 'Use at least 20 characters. The graph will be generated asynchronously.' },
  gameCanvas: { zh: '分支编辑画布', en: 'Branch graph canvas' },
  gameAssets: { zh: '基础组成元素', en: 'Base assets' },
  gameNode: { zh: '视频节点', en: 'Video node' },
  gameEdge: { zh: '选择边', en: 'Choice edge' },
  addChoice: { zh: '＋ 新增选项', en: '＋ Add choice' },
  saveChanges: { zh: '保存修改', en: 'Save changes' },
  generateNodeVideo: { zh: '生成节点视频', en: 'Generate node video' },
  deleteChoice: { zh: '删除选项', en: 'Delete choice' },
  reloadGraph: { zh: '刷新图谱', en: 'Refresh graph' },
  playGame: { zh: '试玩游戏', en: 'Play game' },
  downloadGameDemo: { zh: '下载 Mac 试玩包', en: 'Download Mac demo' },
  emptyDescription: { zh: '这个工作区即将开放，先从短剧生成开始创作吧。', en: 'This workspace is coming soon. Start with drama creation.' },
  enterDrama: { zh: '进入短剧创作', en: 'Open Drama Studio' },
  themeLight: { zh: '切换到浅色模式', en: 'Switch to light mode' },
  themeDark: { zh: '切换到深色模式', en: 'Switch to dark mode' },
  language: { zh: '切换为英文', en: 'Switch to Chinese' },
  collapseSidebar: { zh: '收起侧栏', en: 'Collapse sidebar' },
  expandSidebar: { zh: '展开侧栏', en: 'Expand sidebar' },
  languageModel: { zh: '语言模型', en: 'Language Model' },
  multimodalModel: { zh: '多模态模型', en: 'Multimodal Model' },
  languageModelDescription: { zh: '用于剧本拆解、分镜规划与提示词生成', en: 'For script decomposition, shot planning, and prompt generation' },
  multimodalModelDescription: { zh: '用于生成角色、场景和道具图片', en: 'For character, scene, and prop images' },
  audioModelDescription: { zh: '用于角色配音、旁白和背景音频生成', en: 'For character voices, narration, and background audio' },
  saveModelConfig: { zh: '嗅探调用并保存配置', en: 'Probe and save configuration' },
} as const;
let locale: Locale = localStorage.getItem('locale') === 'en' ? 'en' : 'zh';
let darkMode = localStorage.getItem('theme') === 'dark';
let sidebarCollapsed = localStorage.getItem('sidebarCollapsed') === 'true';
function ui(key: keyof typeof copy) { return copy[key][locale]; }
const navLabels: Record<string, { zh: string; en: string }> = {
  home: { zh: '首页', en: 'Home' }, drama: { zh: '短剧生成', en: 'Drama' }, interactiveGame: { zh: '互动游戏生成', en: 'Interactive Game' }, settings: { zh: '配置', en: 'Settings' },
};
function navLabel(key: string) { return navLabels[key]?.[locale] || key; }
function statusLabel(status: string) { return locale === 'en' ? ({ 草稿: 'Draft', 生成中: 'Generating', 生成成功: 'Succeeded', 生成失败: 'Failed' }[status] || status) : status; }
const projects: Project[] = [];
const nav = [ ['⌂','home'], ['✦','drama'], ['◉','interactiveGame'], ['⚙','settings'] ] as const;
let active = 'drama';
type AppRoute = { page: 'home' | 'drama' | 'interactiveGame' | 'settings'; id?: string };
function parseRoute(): AppRoute {
  const parts = window.location.pathname.split('/').filter(Boolean).map(value => decodeURIComponent(value));
  if (parts[0] === 'drama') return { page: 'drama', id: parts[1] };
  if (parts[0] === 'interactive-game') return { page: 'interactiveGame', id: parts[1] };
  if (parts[0] === 'settings') return { page: 'settings' };
  if (parts[0] === 'home') return { page: 'home' };
  return { page: 'drama' };
}
function routePath(route: AppRoute) { const prefix = route.page === 'interactiveGame' ? '/interactive-game' : `/${route.page}`; return route.id ? `${prefix}/${encodeURIComponent(route.id)}` : prefix; }
function navigate(route: AppRoute, replace = false) { const path = routePath(route); if (window.location.pathname !== path) window.history[replace ? 'replaceState' : 'pushState']({}, '', path); applyRoute(route); }
function applyRoute(route: AppRoute, initial = false) {
  if (initial && window.location.pathname === '/') window.history.replaceState({}, '', routePath(route));
  active = route.page;
  if (route.page === 'drama' && route.id) {
    const changed = dramaViewState.projectId !== route.id;
    dramaViewState.projectId = route.id;
    if (changed) { dramaViewState.shotId = null; dramaViewState.assetPanel = null; dramaViewState.videoUrl = null; }
    // A hard refresh starts with an empty #app, so create the shell/main node
    // before the detail loader tries to replace its contents.
    render();
    void loadDramaDetailCore(route.id);
    return;
  }
  if (route.page === 'interactiveGame' && route.id) { render(); void gameDetailCore(route.id); return; }
  dramaViewState.projectId = null;
  dramaViewState.shotId = null;
  dramaViewState.assetPanel = null;
  dramaViewState.videoUrl = null;
  render();
  if (route.page === 'drama') void loadDramaProjects();
  if (route.page === 'interactiveGame') void loadInteractiveGames();
  if (route.page === 'settings') { void loadModelSettings(); void loadVoicePresets(true); }
}
async function loadDramaDetail(id: string, retry = 0) { if (parseRoute().page !== 'drama' || parseRoute().id !== id) { navigate({ page: 'drama', id }); return; } await loadDramaDetailCore(id, retry); }
async function gameDetail(id: string, initial?: Game) { if (parseRoute().page !== 'interactiveGame' || parseRoute().id !== id) { navigate({ page: 'interactiveGame', id }); return; } await gameDetailCore(id, initial); }
const app = document.querySelector<HTMLDivElement>('#app')!;
const defaultModelSettings: Record<ModelKind, ModelSettings> = {
  language: { kind: 'language', endpoint: '', model: 'doubao-seed', models: ['doubao-seed', 'gpt-4o-mini'] },
  multimodal: { kind: 'multimodal', endpoint: '', model: 'doubao-seeddream', models: ['doubao-seeddream', 'gpt-image-1'] },
  video: { kind: 'video', endpoint: '', create_url: 'https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks', query_url: 'https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks/{id}', model: 'doubao-seedance-2.0', models: ['doubao-seedance-2.0', 'sora-2'] },
  audio: { kind: 'audio', endpoint: '', model: 'doubao-voice', models: ['doubao-voice', 'gpt-4o-mini-tts'] },
};
let modelSettings: Record<ModelKind, ModelSettings> = {
  language: { ...defaultModelSettings.language },
  multimodal: { ...defaultModelSettings.multimodal },
  video: { ...defaultModelSettings.video },
  audio: { ...defaultModelSettings.audio },
};
let voicePresets: VoicePreset[] = [];
let voicePresetsLoaded = false;

configureSettingsRuntime({
  apiBaseUrl: API_BASE_URL,
  modelSettings,
  defaultModelSettings,
  getLocale: () => locale,
  getVoicePresets: () => voicePresets,
  setVoicePresets: items => { voicePresets = items; },
  getVoicePresetsLoaded: () => voicePresetsLoaded,
  setVoicePresetsLoaded: loaded => { voicePresetsLoaded = loaded; },
  isSettingsActive: () => active === 'settings',
  render,
  escapeHtml,
  ui: key => ui(key as keyof typeof copy),
});

configureGameRuntime({
  apiBaseUrl: API_BASE_URL,
  locale: () => locale,
  active: () => active,
  ui: key => ui(key as keyof typeof copy),
  escapeHtml,
  resolveMediaUrl: value => resolveMediaUrl(value),
  applyModelSelect,
  loadModelSettings,
  render,
  toast,
  deleteInteractiveGame,
  setGenerationButtonLoading,
  navigateToGameDetail: id => navigate({ page: 'interactiveGame', id }),
  navigateToGameList: () => navigate({ page: 'interactiveGame' }),
});

dramaCore.configureDramaRuntime({
  apiBaseUrl: API_BASE_URL,
  active: () => active,
  projects,
  projectFromApi,
  render,
  escapeHtml,
  toast,
  loadDramaDetail,
  loadDramaProjects,
  loadVoicePresets,
  voiceOptions,
  voicePreset,
  bindDramaWorkspace,
});

configureDramaPartialRefresh({ apiBaseUrl: API_BASE_URL, loadFullDetail: loadDramaDetail });
configureDramaTaskPolling({ apiBaseUrl: API_BASE_URL, getProject: () => active === 'drama' && dramaViewState.projectId === activeDramaProject?.id ? activeDramaProject : null, onStatusUpdate: applyDramaTaskUpdate });

dramaModal.configureDramaModalRuntime({
  apiBaseUrl: API_BASE_URL,
  projects,
  projectFromApi,
  render,
  escapeHtml,
  toast,
  loadDramaDetail,
  loadDramaProjects,
  loadModelSettings,
  applyModelSelect,
  deleteDramaProject,
});

dramaShot.configureDramaShotRuntime({
  apiBaseUrl: API_BASE_URL,
  toast,
  loadDramaDetail,
});

function openConfiguredDramaModal() {
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop';
  modal.id = 'drama-create-modal';
  modal.innerHTML = `<div class="modal drama-create-modal configured-drama-modal"><button class="close" data-drama-modal-close>×</button><div class="modal-head"><div class="eyebrow">DRAMA PROJECT / NEW</div><h2>新建短剧</h2><p>先上传或粘贴剧本内容，再补充短剧的基础配置。项目会立即创建，分镜会在后台提取并回填。</p></div><div class="drama-create-stepper"><span class="active" data-step-label="1">1 文本来源</span><span data-step-label="2">2 基础配置</span></div><section id="drama-create-step-1"><h3>选择文本来源</h3><div class="drama-source-tabs"><label><input type="file" id="configured-drama-file" accept=".txt,.md,.text" />⌃ 上传文件</label><button class="active" id="configured-drama-paste-tab">▣ 粘贴文本</button></div><label>粘贴文本内容 <span id="configured-drama-char-count">0 字</span><textarea id="configured-drama-script" rows="11" placeholder="请将小说、剧本等文本内容粘贴到此处..."></textarea><div class="hint">剧本文本不少于 10 个字，创建后会异步拆解为分镜、角色、场景和道具。</div><div class="modal-actions"><button class="ghost" data-drama-modal-close>取消</button><button class="primary" id="configured-drama-next">下一步 →</button></div></section><section id="drama-create-step-2" hidden><h3>短剧基础配置</h3><label>项目名称 <em>*</em><input id="configured-drama-name" placeholder="建议使用书名 / 剧名 + 集数 / 部分命名" /></label><div class="form-grid"><label>生成视频的比例<select id="configured-drama-ratio"><option selected>9:16</option><option>16:9</option></select></label><label>视频风格<select id="configured-drama-style"><option selected>真人风格</option><option>2D动漫风</option><option>3D动漫风</option><option value="自定义">自定义</option></select><input id="configured-drama-style-custom" class="drama-custom-option" hidden placeholder="输入自定义视频风格" /></label><label>叙述的背景主题<select id="configured-drama-theme"><option selected>都市</option><option>悬疑</option><option>科幻</option><option>古风</option><option>玄幻</option><option>年代农村剧</option><option value="自定义">自定义</option></select><input id="configured-drama-theme-custom" class="drama-custom-option" hidden placeholder="输入自定义背景主题" /></label><label>语言模型<select id="configured-drama-language-model" disabled><option>正在读取设置…</option></select></label><label>图像模型<select id="configured-drama-image-model" disabled><option>正在读取设置…</option></select></label><label>视频模型<select id="configured-drama-video-model" disabled><option>正在读取设置…</option></select></label><label>短剧分辨率<select id="configured-drama-resolution"><option selected>720p</option><option>480p</option></select></label></div><div class="drama-constraint-section"><h3>分镜组成元素约束</h3><p class="hint">创建时选中的约束会应用到所有分镜，后续仍可在全局参数中调整。</p><div class="form-grid"><label>字幕<select id="configured-drama-subtitles"><option value="false" selected>不要字幕</option><option value="true">需要字幕</option></select></label><label>背景音乐<select id="configured-drama-background-music"><option value="false" selected>不要背景音乐</option><option value="true">需要背景音乐</option></select></label></div></div><div class="modal-actions"><button class="ghost" id="configured-drama-prev">← 上一步</button><button class="ghost" data-drama-modal-close>取消</button><button class="primary" id="configured-drama-create" disabled>正在读取模型配置…</button></div></section></div>`;
  document.body.append(modal);
  const createButton = modal.querySelector<HTMLButtonElement>('#configured-drama-create')!;
  const modelsReady = loadModelSettings().then(loaded => {
    if (!modal.isConnected) return loaded;
    applyModelSelect(modal, '#configured-drama-language-model', 'language');
    applyModelSelect(modal, '#configured-drama-image-model', 'multimodal');
    applyModelSelect(modal, '#configured-drama-video-model', 'video');
    modal.querySelectorAll<HTMLSelectElement>('#configured-drama-language-model,#configured-drama-image-model,#configured-drama-video-model').forEach(select => { select.disabled = !loaded; });
    createButton.disabled = !loaded;
    createButton.textContent = loaded ? '创建项目' : '模型配置加载失败';
    return loaded;
  });
  const close = () => modal.remove();
  modal.querySelectorAll<HTMLElement>('[data-drama-modal-close]').forEach(element => element.addEventListener('click', close));
  const script = modal.querySelector<HTMLTextAreaElement>('#configured-drama-script')!;
  const count = modal.querySelector('#configured-drama-char-count')!;
  script.addEventListener('input', () => { count.textContent = `${script.value.length} 字`; });
  modal.querySelector<HTMLInputElement>('#configured-drama-file')?.addEventListener('change', async event => { const file = (event.target as HTMLInputElement).files?.[0]; if (file) { script.value = await file.text(); script.dispatchEvent(new Event('input')); } });
  const styleSelect = modal.querySelector<HTMLSelectElement>('#configured-drama-style')!;
  const themeSelect = modal.querySelector<HTMLSelectElement>('#configured-drama-theme')!;
  const syncCustom = (select: HTMLSelectElement, inputId: string) => { const input = modal.querySelector<HTMLInputElement>(`#${inputId}`)!; input.hidden = select.value !== '自定义'; if (input.hidden) input.value = ''; };
  styleSelect.addEventListener('change', () => syncCustom(styleSelect, 'configured-drama-style-custom'));
  themeSelect.addEventListener('change', () => syncCustom(themeSelect, 'configured-drama-theme-custom'));
  const readValue = (id: string) => (modal.querySelector(`#${id}`) as HTMLInputElement | HTMLSelectElement).value;
  const readCustomValue = (selectId: string, inputId: string) => { const selected = readValue(selectId); return selected === '自定义' ? readValue(inputId).trim() || '自定义' : selected; };
  modal.querySelector('#configured-drama-next')?.addEventListener('click', () => { if (script.value.trim().length < 10) { toast('剧本文本不少于 10 个字'); return; } (modal.querySelector('#drama-create-step-1') as HTMLElement).hidden = true; (modal.querySelector('#drama-create-step-2') as HTMLElement).hidden = false; modal.querySelector('[data-step-label="1"]')?.classList.remove('active'); modal.querySelector('[data-step-label="2"]')?.classList.add('active'); });
  modal.querySelector('#configured-drama-prev')?.addEventListener('click', () => { (modal.querySelector('#drama-create-step-1') as HTMLElement).hidden = false; (modal.querySelector('#drama-create-step-2') as HTMLElement).hidden = true; modal.querySelector('[data-step-label="1"]')?.classList.add('active'); modal.querySelector('[data-step-label="2"]')?.classList.remove('active'); });
  modal.querySelector('#configured-drama-create')?.addEventListener('click', async () => { if (!await modelsReady) { toast('模型配置加载失败，请检查设置服务后重试'); return; } const name = readValue('configured-drama-name').trim(); if (!name) { toast('请填写项目名称'); return; } const button = modal.querySelector<HTMLButtonElement>('#configured-drama-create')!; button.disabled = true; button.textContent = '创建中…'; try { const response = await fetch(`${API_BASE_URL}/projects`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, script: script.value.trim(), ratio: readValue('configured-drama-ratio'), style: readCustomValue('configured-drama-style', 'configured-drama-style-custom'), theme: readCustomValue('configured-drama-theme', 'configured-drama-theme-custom'), language_model: readValue('configured-drama-language-model'), multimodal_model: readValue('configured-drama-image-model'), video_model: readValue('configured-drama-video-model'), resolution: readValue('configured-drama-resolution'), shot_constraints: { subtitles: readValue('configured-drama-subtitles') === 'true', background_music: readValue('configured-drama-background-music') === 'true' } }) }); if (!response.ok) throw new Error(`HTTP ${response.status}`); const project = await response.json() as ApiProject; projects.unshift(projectFromApi(project)); close(); dramaViewState.shotId = null; dramaViewState.videoUrl = null; void loadDramaDetail(project.id); } catch (error) { button.disabled = false; button.textContent = '创建项目'; toast('创建失败，请确认后端已启动'); console.error(error); } });
}

// This capture listener is registered before the legacy modal listener below.
// It keeps the existing editor behavior intact while using the full creation form.
document.addEventListener('click', event => { const target = event.target instanceof HTMLElement ? event.target : null; if (!target?.closest('#new-project')) return; event.preventDefault(); event.stopImmediatePropagation(); openConfiguredDramaModal(); }, true);

function openCurrentShotReferencePicker() {
  if (!dramaViewState.projectId) return;
  const projectId = dramaViewState.projectId;
  void fetch(`${API_BASE_URL}/projects/${projectId}`).then(response => {
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    return response.json() as Promise<ApiProject>;
  }).then(project => {
    const shot = dramaSelectedShot(project);
    if (!shot) throw new Error('当前没有可编辑的分镜');
    // The chooser is usable even during the rich-editor initialization window.
    // If the editor is missing, initialize it here and keep a textarea fallback.
    if (!document.querySelector('.drama-rich-prompt-editor')) setupDramaRichPromptEditor(project, shot);
    const editor = document.querySelector<HTMLElement>('.drama-rich-prompt-editor');
    const promptInput = document.querySelector<HTMLTextAreaElement>('#drama-shot-prompt');
    let existingNodes: DramaPromptNode[] = [];
    if (editor) existingNodes = readDramaPromptNodes(editor);
    else if (Array.isArray(shot.prompt_rich) && shot.prompt_rich.length) existingNodes = shot.prompt_rich;
    else if (shot.prompt) existingNodes = [{ type: 'text', text: shot.prompt }];
    openDramaReferencePicker(project, existingNodes, nodes => {
      const currentEditor = document.querySelector<HTMLElement>('.drama-rich-prompt-editor');
      if (currentEditor) {
        appendDramaReferenceNodes(project, nodes);
        const currentShot = dramaSelectedShot(project);
        if (currentShot) currentShot.prompt_rich = readDramaPromptNodes(currentEditor);
        setActiveDramaProject(project);
        syncDramaShotReferencePanel(project);
        return;
      }
      if (!promptInput) return;
      const serialized = serializeDramaPromptNodes(project, [...existingNodes, ...nodes]);
      promptInput.value = serialized.prompt;
      promptInput.dataset.promptRich = JSON.stringify(serialized.nodes);
      const currentShot = dramaSelectedShot(project);
      if (currentShot) currentShot.prompt_rich = serialized.nodes;
      setActiveDramaProject(project);
      syncDramaShotReferencePanel(project);
      toast(`已添加 ${nodes.length} 项参考图，请使用右上角保存`);
    });
  }).catch(error => { toast('参考图加载失败'); console.error(error); });
}

// Keep this listener ahead of the legacy handler below so the chooser never
// depends on the editor DOM being ready at the exact click moment.
document.addEventListener('click', event => {
  const target = event.target instanceof HTMLElement ? event.target : null;
  if (!target?.closest('[data-drama-add-reference]')) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  openCurrentShotReferencePicker();
}, true);

function applyTheme() { document.documentElement.dataset.theme = darkMode ? 'dark' : 'light'; }
function render() { applyTheme(); const sidebarLabel = sidebarCollapsed ? ui('expandSidebar') : ui('collapseSidebar'); app.innerHTML = `<div class="shell"><aside class="${sidebarCollapsed?'collapsed':''}"><div class="brand"><span class="brand-mark">✦</span><span>造梦工厂</span></div><div class="nav">${nav.map(([icon,key]) => `<button class="nav-item ${active===key?'active':''}" data-nav="${key}"><i>${icon}</i>${navLabel(key)}${key==='drama'?'<b>Beta</b>':''}</button>`).join('')}</div><div class="aside-bottom"><span class="ready-dot"></span><span>${ui('ready')}</span><div class="aside-tools"><button class="tool-button" id="sidebar-toggle" title="${sidebarLabel}" aria-label="${sidebarLabel}" aria-pressed="${sidebarCollapsed}">${sidebarCollapsed?'›':'‹'}</button><button class="tool-button" id="theme-toggle" title="${darkMode?ui('themeLight'):ui('themeDark')}" aria-label="${darkMode?ui('themeLight'):ui('themeDark')}">${darkMode?'☼':'☾'}</button><button class="tool-button language-button" id="language-toggle" title="${ui('language')}" aria-label="${ui('language')}">文 <small>${locale==='zh'?'EN':'中'}</small></button></div></div></aside><main>${active==='drama'?dramaPage():active==='interactiveGame'?interactiveGamePage():active==='settings'?settingsPage():emptyPage()}</main></div>`; bind(); }
function dramaPage() { return `<header><div><div class="eyebrow">${ui('workspace')}</div><h1>${ui('dramaTitle')}</h1><p>${ui('dramaDescription')}</p></div><button class="primary" id="new-project">${ui('newDrama')}</button></header><section class="toolbar"><div class="search">⌕ <input placeholder="${ui('search')}" /></div><button class="ghost">${ui('refresh')}</button><span class="toolbar-count">${projects.length} ${ui('projects')}</span></section><section class="cards">${projects.map(projectCard).join('')}</section>`; }
function projectFromApi(p: ApiProject): Project { const assets=p.assets||[]; return { id:p.id, name:p.name, status:p.status, ratio:p.ratio, style:p.style, theme:p.theme, createdAt:p.created_at?.slice(0,16).replace('T',' ')||'刚刚', scenes:p.shots?.length||0, characters:assets.filter(a=>a.type==='character').length, locations:assets.filter(a=>a.type==='scene').length, props:assets.filter(a=>a.type==='prop').length }; }
function projectCard(p: Project) { return `<article class="project-card" data-project="${p.id}"><div class="card-top"><h2>${escapeHtml(p.name)}</h2><span class="status ${p.status==='生成中'?'running':''}">${p.status==='生成中'?'◌ ':''}${statusLabel(p.status)}</span><div class="tags"><span>${escapeHtml(p.ratio)}</span><span>${escapeHtml(p.style)}</span><span>${escapeHtml(p.theme)}</span></div></div><div class="metrics"><div><strong>${p.scenes}</strong><small>${locale==='en'?'Shots':'分镜'}</small></div><div><strong>${p.characters}</strong><small>${locale==='en'?'Roles':'角色'}</small></div><div><strong>${p.locations}</strong><small>${locale==='en'?'Scenes':'场景'}</small></div><div><strong>${p.props}</strong><small>${locale==='en'?'Props':'道具'}</small></div></div><div class="card-foot"><span>${escapeHtml(p.createdAt)}</span><button type="button" class="delete-card-button" data-delete-project="${p.id}">删除</button><span aria-hidden="true">›</span></div></article>`; }
function settingsPage() { return `<header class="settings-page-header"><div><div class="eyebrow">${ui('settingsEyebrow')}</div><h1>${ui('settingsTitle')}</h1><p>${ui('settingsDescription')}</p></div></header><section class="settings-grid">${modelSettingsCard('language', ui('languageModel'), ui('languageModelDescription'), '文', ui('saveModelConfig'))}${modelSettingsCard('multimodal', '图像模型', ui('multimodalModelDescription'), '✧', ui('saveModelConfig'))}${modelSettingsCard('video', '视频模型', '用于生成分镜视频和互动游戏节点视频', '▣', ui('saveModelConfig'))}${modelSettingsCard('audio', '音频模型', ui('audioModelDescription'), '♫', ui('saveModelConfig'))}${voiceCatalogCard()}</section>`; }
function emptyPage() { const item=nav.find(n=>n[1]===active); const isInteractiveGame=active==='interactiveGame'; return `<div class="empty"><div class="empty-icon">${item?.[0]||'✦'}</div><h1>${navLabel(active)}</h1><p>${isInteractiveGame?ui('interactiveGameDescription'):ui('emptyDescription')}</p>${isInteractiveGame?'':`<button class="primary" data-nav="drama">${ui('enterDrama')}</button>`}</div>`; }
function updateSidebarToggle() { const aside=document.querySelector('aside'); const button=document.querySelector<HTMLButtonElement>('#sidebar-toggle'); const label=sidebarCollapsed?ui('expandSidebar'):ui('collapseSidebar'); aside?.classList.toggle('collapsed',sidebarCollapsed); if(button){button.textContent=sidebarCollapsed?'›':'‹';button.title=label;button.setAttribute('aria-label',label);button.setAttribute('aria-pressed',String(sidebarCollapsed));} }
function toggleSidebar() { sidebarCollapsed=!sidebarCollapsed; localStorage.setItem('sidebarCollapsed',String(sidebarCollapsed)); updateSidebarToggle(); }
function toggleTheme() { darkMode=!darkMode; localStorage.setItem('theme',darkMode?'dark':'light'); applyTheme(); render(); }
function toggleLanguage() { locale=locale==='zh'?'en':'zh'; localStorage.setItem('locale',locale); render(); }
function bind() { document.querySelectorAll<HTMLElement>('[data-nav]').forEach(el=>el.onclick=()=>navigate({ page: (el.dataset.nav || 'drama') as AppRoute['page'] })); document.querySelector('#new-project')?.addEventListener('click',openConfiguredDramaModal); document.querySelector('#new-game')?.addEventListener('click',openGameModal); document.querySelector('#new-game-empty')?.addEventListener('click',openGameModal); document.querySelector('#refresh-games')?.addEventListener('click',()=>void loadInteractiveGames()); document.querySelector('#sidebar-toggle')?.addEventListener('click',toggleSidebar); document.querySelector('#theme-toggle')?.addEventListener('click',toggleTheme); document.querySelector('#language-toggle')?.addEventListener('click',toggleLanguage); document.querySelectorAll<HTMLElement>('[data-project]').forEach(el=>el.onclick=()=>void loadDramaDetail(el.dataset.project!)); document.querySelectorAll<HTMLElement>('[data-game]').forEach(el=>el.onclick=()=>void gameDetail(el.dataset.game!)); document.querySelectorAll<HTMLElement>('[data-delete-game]').forEach(el=>el.onclick=event=>{event.preventDefault();event.stopPropagation();void deleteInteractiveGame(el.dataset.deleteGame||'');}); }
function openModal() { const modal=document.createElement('div'); modal.className='modal-backdrop'; modal.innerHTML=`<div class="modal"><button class="close">×</button><div class="modal-head"><div class="eyebrow">PROJECT / NEW</div><h2>新建短剧</h2><p>先上传或粘贴剧本内容，再补充短剧的基础配置。项目会立即创建，分镜会在后台提取并回填。</p></div><div class="stepper"><span class="current">1 文本来源</span><span>2 基础配置</span></div><label>项目名称 <em>*</em><input id="project-name" placeholder="建议使用书名 / 剧名 + 集数 / 部分来命名" /></label><label>剧本内容 <em>*</em><textarea id="script" placeholder="请将小说、剧本等文本内容粘贴到此处..." rows="7"></textarea><div class="hint">剧本文本不少于 10 个字，创建后会异步拆解为分镜、角色、场景和道具。</div><div class="modal-actions"><button class="ghost close-action">取消</button><button class="primary" id="create">创建并进入详情</button></div></div>`; document.body.append(modal); const close=()=>modal.remove(); modal.querySelectorAll('.close,.close-action').forEach(x=>x.addEventListener('click',close)); modal.querySelector('#create')?.addEventListener('click',async()=>{ const name=(modal.querySelector('#project-name') as HTMLInputElement).value.trim()||'未命名短剧'; const script=(modal.querySelector('#script') as HTMLTextAreaElement).value.trim(); if(script.length<10){toast('剧本文本不少于 10 个字');return;} const button=modal.querySelector('#create') as HTMLButtonElement; button.disabled=true; button.textContent='创建中…'; try { const response=await fetch(`${API_BASE_URL}/projects`,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({name,script,ratio:'9:16',style:'真人风格',theme:'都市',language_model:'doubao-seed',multimodal_model:'doubao-seeddream'})}); if(!response.ok) throw new Error(`HTTP ${response.status}`); const project=await response.json() as ApiProject; projects.unshift(projectFromApi(project)); close(); detail(project.id,project.name); } catch(error) { button.disabled=false; button.textContent='创建并进入详情'; toast('创建失败，请确认后端已启动'); console.error(error); } }); }
function detail(id: string, name?: string) { const project=projects.find(p=>p.id===id); document.querySelector('main')!.innerHTML=`<div class="detail"><button class="back" id="back">← 返回项目列表</button><header><div><div class="eyebrow">短剧项目 / ${name||project?.name||'金丝雀'}</div><h1>${name||project?.name||'金丝雀'}</h1><p>项目正在后台生成结构化素材，完成后可逐项生成图片和视频。</p></div><button class="primary" id="generate-video">▣ 生成所有视频</button></header><div class="detail-grid"><section class="panel episode-panel"><div class="panel-title"><h2>剧集 / 分镜</h2><button class="ghost">＋ 新增剧集</button></div>${['第1集','第2集','第3集'].map((x,i)=>`<div class="episode"><b>${x}</b><span>${i===0?'1':'0'} 条分镜　⌄</span>${i===0?'<div class="shot selected"><b>#1</b><span class="status running">生成中</span><p>正在提取分镜脚本与基础组成元素...</p></div>':''}</div>`).join('')}</section><section class="panel editor"><div class="panel-title"><div><h2>分镜编辑</h2><p>第1集 · 当前第1条</p></div><button class="primary" id="generate-shot">生成视频</button></div><label>分镜标题<input value="森林中的第一束光" /></label><label>分镜文本<textarea rows="5">${name||'金丝雀'}在森林深处醒来，阳光穿过树叶洒下金色光斑。镜头缓慢推进，鸟儿展开翅膀。</textarea></label><div class="asset-section"><div class="section-title"><h3>基础组成元素</h3><button class="ghost" id="generate-assets">批量生成图片</button></div><div class="asset-tabs"><button class="tab active">角色</button><button class="tab">场景</button><button class="tab">道具</button></div><div class="asset-row"><div class="asset-thumb">◌</div><div><b>金丝雀</b><span>角色 · 基础形态</span><p>人形金丝雀，金色羽毛，温柔而坚定的眼神</p></div><button class="small-btn">生成图片</button></div><div class="asset-row"><div class="asset-thumb green">✦</div><div><b>森林橡树枝桠木</b><span>场景 · 默认形态</span><p>阳光穿过茂密树冠，空气中有微小尘埃</p></div><button class="small-btn">生成图片</button></div></div></section><section class="panel preview"><h2>视频预览</h2><div class="video-placeholder"><div>✦</div><span>生成视频后将在这里预览</span></div><h3>视频生成 Prompt</h3><div class="prompt-box">场景：森林橡树枝桠木<br/>角色：金丝雀（基础形态）<br/>风格：2D动漫风 · 光线：强光暖调<br/>镜头：中景，平视固定镜头，持续 10s</div></section></div></div>`; document.querySelector('#back')?.addEventListener('click',render); document.querySelector('#generate-video')?.addEventListener('click',()=>toast('视频生成任务已创建，后台处理中')); document.querySelector('#generate-shot')?.addEventListener('click',()=>toast('分镜视频任务已创建')); document.querySelector('#generate-assets')?.addEventListener('click',()=>toast('已创建 2 个图片生成任务')); }


document.addEventListener('click', event => { const target = event.target as HTMLElement; const deleteProjectButton = target.closest<HTMLElement>('[data-delete-project]'); if (deleteProjectButton) { event.preventDefault(); event.stopImmediatePropagation(); void deleteDramaProject(deleteProjectButton.dataset.deleteProject || ''); return; } const newProject = target.closest('#new-project'); const projectCardElement = target.closest<HTMLElement>('[data-project]'); if (newProject || projectCardElement) { event.preventDefault(); event.stopPropagation(); if (newProject) openConfiguredDramaModal(); else if (projectCardElement?.dataset.project) void loadDramaDetail(projectCardElement.dataset.project); } }, true);
document.addEventListener('click', event => {
  const target = event.target instanceof HTMLElement ? event.target : null;
  const button = target?.closest<HTMLButtonElement>('[data-model-api-key-toggle]');
  if (!button) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  const card = button.closest<HTMLElement>('[data-model-config-card]');
  const kind = card?.dataset.modelKind as ModelKind | undefined;
  const input = card?.querySelector<HTMLInputElement>('[data-model-api-key]');
  if (!card || !kind || !input) return;
  if (input.type === 'text') {
    input.type = 'password';
    button.innerHTML = apiKeyVisibilityIcon(false);
    button.title = '查看 API Key';
    button.setAttribute('aria-label', '查看 API Key');
    return;
  }
  button.disabled = true;
  void fetch(`${API_BASE_URL}/settings/models/${kind}/api-key`)
    .then(async response => {
      const payload = await response.json().catch(() => ({})) as { api_key?: string; detail?: string };
      if (!response.ok || !payload.api_key) throw new Error(payload.detail || `HTTP ${response.status}`);
      input.value = payload.api_key;
      input.type = 'text';
      button.innerHTML = apiKeyVisibilityIcon(true);
      button.title = '隐藏 API Key';
      button.setAttribute('aria-label', '隐藏 API Key');
    })
    .catch(error => toast(`API Key 读取失败：${error instanceof Error ? error.message : '请检查配置'}`))
    .finally(() => { button.disabled = false; });
}, true);
document.addEventListener('click', event => { const target = event.target instanceof HTMLElement ? event.target : null; const button = target?.closest<HTMLButtonElement>('[data-save-model-config]'); if (!button) return; event.preventDefault(); event.stopPropagation(); const card = button.closest<HTMLElement>('[data-model-config-card]'); const kind = card?.dataset.modelKind as ModelKind | undefined; if (!card || !kind) return; const endpoint = card.querySelector<HTMLInputElement>('[data-model-endpoint]')?.value.trim() || ''; const createUrl = card.querySelector<HTMLInputElement>('[data-model-create-url]')?.value.trim() || ''; const queryUrl = card.querySelector<HTMLInputElement>('[data-model-query-url]')?.value.trim() || ''; const apiKey = card.querySelector<HTMLInputElement>('[data-model-api-key]')?.value.trim() || ''; const models = modelEditorValues(card); const defaultModel = card.querySelector<HTMLElement>('[data-model-selected]')?.dataset.modelSelected?.trim() || models[0] || ''; if (defaultModel && !models.includes(defaultModel)) models.unshift(defaultModel); const idleText = button.textContent || '嗅探调用并保存配置'; button.disabled = true; button.textContent = '⟳ 正在嗅探调用…'; void fetch(`${API_BASE_URL}/settings/models`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ kind, model: defaultModel, models, endpoint, create_url: createUrl, query_url: queryUrl, api_key: apiKey }) }).then(async response => { const payload = await response.json().catch(() => ({})) as { detail?: string }; if (!response.ok) throw new Error(payload.detail || `HTTP ${response.status}`); return payload as ModelSettings; }).then(() => { toast('模型嗅探调用成功，配置已保存'); return loadModelSettings(); }).catch(error => { toast(`模型嗅探失败，配置未保存：${error instanceof Error ? error.message : '请检查配置'}`); console.error(error); }).finally(() => { button.disabled = false; button.textContent = idleText; }); }, true);
document.addEventListener('click', event => { const target = event.target instanceof HTMLElement ? event.target : null; const trigger = target?.closest<HTMLElement>('[data-model-trigger]'); const option = target?.closest<HTMLElement>('[data-model-option]'); const addButton = target?.closest<HTMLButtonElement>('[data-model-add-button]'); const removeButton = target?.closest<HTMLButtonElement>('[data-model-remove]'); const card = target?.closest<HTMLElement>('[data-model-config-card]'); const closeMenus = () => document.querySelectorAll<HTMLElement>('[data-model-menu]').forEach(menu => { menu.hidden = true; menu.closest<HTMLElement>('[data-model-config-card]')?.querySelector<HTMLElement>('[data-model-trigger]')?.setAttribute('aria-expanded', 'false'); }); if (removeButton && card) { event.preventDefault(); event.stopPropagation(); const kind = card.dataset.modelKind as ModelKind; const current = modelEditorValues(card); const value = removeButton.dataset.modelRemove || ''; const selected = card.querySelector<HTMLElement>('[data-model-selected]')?.dataset.modelSelected || ''; const next = current.filter(item => item !== value); renderModelEditor(card, kind, next, selected === value ? next[0] : selected); return; } if (addButton && card) { event.preventDefault(); event.stopPropagation(); const kind = card.dataset.modelKind as ModelKind; const current = modelEditorValues(card); const input = card.querySelector<HTMLInputElement>('[data-model-add]'); const value = input?.value.trim() || ''; if (!value) return; if (!current.includes(value)) current.push(value); if (input) input.value = ''; renderModelEditor(card, kind, current, value); return; } if (option && card) { event.preventDefault(); event.stopPropagation(); const kind = card.dataset.modelKind as ModelKind; const value = option.dataset.modelOption || ''; renderModelEditor(card, kind, modelEditorValues(card), value); return; } if (trigger && card) { event.preventDefault(); event.stopPropagation(); const menu = card.querySelector<HTMLElement>('[data-model-menu]'); if (!menu) return; const open = menu.hidden; closeMenus(); menu.hidden = !open; trigger.setAttribute('aria-expanded', String(!menu.hidden)); return; } if (!target?.closest('[data-model-menu]')) closeMenus(); }, true);
document.addEventListener('change', event => { const target = event.target as HTMLSelectElement; if (target?.id !== 'storage-provider') return; const provider = target.value as StorageSettingsResponse['provider']; const isLocal = provider === 'local'; document.querySelectorAll<HTMLElement>('.storage-settings-form label').forEach(label => { if (label.querySelector('#storage-endpoint,#storage-bucket,#storage-region,#storage-secret-id,#storage-secret-key')) label.classList.toggle('disabled', isLocal); }); const status = document.querySelector<HTMLElement>('#storage-settings-status'); if (status) status.textContent = storageProviderStatus(provider); const endpoint = storageField('storage-endpoint'); if (endpoint) endpoint.placeholder = storageEndpointPlaceholder(provider); }, true);
document.addEventListener('change', event => { const target = event.target instanceof HTMLSelectElement ? event.target : null; const assetId = target?.dataset.dramaVoice; if (!target || !assetId || !dramaViewState.projectId) return; event.preventDefault(); event.stopPropagation(); const projectId = dramaViewState.projectId; target.disabled = true; void fetch(`${API_BASE_URL}/projects/${projectId}/assets/${assetId}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ voice_id: target.value || null }) }).then(async response => { if (!response.ok) throw new Error(`HTTP ${response.status}`); return response.json(); }).then(() => { toast('角色音色已保存'); return loadDramaDetail(projectId); }).catch(error => { toast('角色音色保存失败'); console.error(error); target.disabled = false; }); }, true);
document.addEventListener('click', event => { const target = event.target instanceof HTMLElement ? event.target : null; const button = target?.closest<HTMLButtonElement>('#save-storage-settings'); if (!button) return; event.preventDefault(); event.stopPropagation(); const value = (id: string) => storageField(id)?.value.trim() || ''; const payload = { provider: value('storage-provider'), endpoint: value('storage-endpoint'), bucket: value('storage-bucket'), region: value('storage-region'), secret_id: value('storage-secret-id'), secret_key: value('storage-secret-key'), prefix: value('storage-prefix') || 'media', public_base_url: value('storage-public-base-url') }; const idleText = button.textContent || '嗅探上传并保存配置'; button.disabled = true; button.textContent = payload.provider === 'local' ? '⟳ 正在保存…' : '⟳ 正在嗅探上传与访问…'; void fetch(`${API_BASE_URL}/settings/storage`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload) }).then(async response => { if (!response.ok) { const detail = await response.json().catch(() => ({})); throw new Error(detail.detail || `HTTP ${response.status}`); } return response.json() as Promise<StorageSettingsResponse>; }).then(settings => { const status = document.querySelector<HTMLElement>('#storage-settings-status'); if (status) status.textContent = storageProviderStatus(settings.provider); toast(settings.provider === 'local' ? '媒体存储配置已保存' : '媒体存储上传与访问嗅探成功，配置已保存'); void loadStorageSettings(); }).catch(error => { toast(`媒体存储嗅探失败，配置未保存：${error instanceof Error ? error.message : '请检查配置'}`); console.error(error); }).finally(() => { button.disabled = false; button.textContent = idleText; }); }, true);
document.addEventListener('click', event => { const target = event.target instanceof HTMLElement ? event.target : null; const button = target?.closest('#drama-save-shot'); if (!button || !dramaViewState.projectId) return; event.preventDefault(); event.stopPropagation(); const projectId = dramaViewState.projectId; const title = (document.querySelector('#drama-shot-title') as HTMLInputElement | null)?.value || ''; const originalText = (document.querySelector('#drama-shot-original') as HTMLTextAreaElement | null)?.value || ''; const promptInput = document.querySelector<HTMLTextAreaElement>('#drama-shot-prompt'); let promptRich: DramaPromptNode[] = []; try { promptRich = JSON.parse(promptInput?.dataset.promptRich || '[]') as DramaPromptNode[]; } catch { promptRich = []; } void fetch(`${API_BASE_URL}/projects/${projectId}`).then(response => { if (!response.ok) throw new Error(`HTTP ${response.status}`); return response.json() as Promise<ApiProject>; }).then(project => { const shot = dramaSelectedShot(project); if (!shot) throw new Error('当前没有可保存的分镜'); return fetch(`${API_BASE_URL}/projects/${projectId}/shots/${shot.id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ title, original_text: originalText, prompt: promptInput?.value || '', prompt_rich: promptRich }) }); }).then(response => { if (!response.ok) throw new Error(`HTTP ${response.status}`); toast('分镜富文本提示词已保存'); void loadDramaDetail(projectId); }).catch(error => { toast('分镜保存失败'); console.error(error); }); }, true);
document.addEventListener('click', event => { const target = event.target instanceof HTMLElement ? event.target : null; const button = target?.closest('[data-drama-open-asset-public]'); if (!button || !dramaViewState.projectId || !dramaViewState.assetPanel) return; event.preventDefault(); event.stopPropagation(); const kind = dramaViewState.assetPanel; void fetch(`${API_BASE_URL}/projects/${dramaViewState.projectId}`).then(response => { if (!response.ok) throw new Error(`HTTP ${response.status}`); return response.json() as Promise<ApiProject>; }).then(project => openAssetPublicPromptModal(project, kind)).catch(error => { toast('公共提示词加载失败'); console.error(error); }); }, true);
document.addEventListener('click', event => { const target = event.target instanceof HTMLElement ? event.target : null; const button = target?.closest('.toolbar .ghost'); if (button && active === 'drama' && !button.id) { event.preventDefault(); event.stopPropagation(); void loadDramaProjects(); } }, true);
document.addEventListener('click', event => { const target = event.target instanceof HTMLElement ? event.target : null; const navButton = target?.closest<HTMLElement>('[data-nav]'); if (!navButton?.dataset.nav) return; event.preventDefault(); event.stopPropagation(); navigate({ page: navButton.dataset.nav as AppRoute['page'] }); }, true);
document.addEventListener('click', event => { const target = event.target instanceof HTMLElement ? event.target : null; if (!target?.closest('#drama-back')) return; event.preventDefault(); event.stopPropagation(); navigate({ page: 'drama' }); }, true);
document.addEventListener('click', event => { const target = event.target instanceof HTMLElement ? event.target : null; if (!target?.closest('#game-back')) return; event.preventDefault(); event.stopPropagation(); navigate({ page: 'interactiveGame' }); }, true);
function escapeHtml(value: unknown) { return String(value??'').replace(/[&<>'"]/g,character=>({'&':'&amp;','<':'&lt;','>':'&gt;',"'":'&#39;','\"':'&quot;'}[character]||character)); }
function toast(message:string){const t=document.createElement('div');t.className='toast';t.textContent='✓ '+message;document.body.append(t);setTimeout(()=>t.remove(),2600)}
function openDramaReferencePicker(project: ApiProject, existingNodes: DramaPromptNode[], onComplete: (nodes: DramaPromptNode[]) => void) { type PickerKind = DramaPromptAssetType; let activeKind: PickerKind = 'character'; const selected = new Set<string>(); const existing = new Set(existingNodes.filter(node => node.type === 'reference').map(node => node.asset_id)); const nodeById = new Map<string, DramaPromptNode>(); const modal = document.createElement('div'); modal.className = 'modal-backdrop drama-reference-picker-backdrop'; modal.innerHTML = `<div class="modal drama-reference-picker"><button class="close" aria-label="关闭">×</button><div class="modal-head"><h2>添加参考图</h2><p>选择素材加入当前分镜参考图。</p></div><div class="drama-reference-picker-tabs">${(['character', 'scene', 'prop', 'placeholder'] as PickerKind[]).map(kind => `<button type="button" class="${kind === activeKind ? 'active' : ''}" data-reference-kind="${kind}">${kind === 'character' ? '角色' : kind === 'scene' ? '场景' : kind === 'prop' ? '道具' : '占位图'}</button>`).join('')}</div><div class="drama-reference-picker-body"></div><div class="drama-reference-picker-actions"><span class="drama-reference-picker-count">已选择 0 项</span><button type="button" class="ghost" data-reference-cancel>取消</button><button type="button" class="primary" data-reference-complete>完成</button></div></div>`; document.body.append(modal); const picker = modal.querySelector<HTMLElement>('.drama-reference-picker')!; const body = modal.querySelector<HTMLElement>('.drama-reference-picker-body')!; const getOptions = () => dramaAssets(project).filter(asset => asset.type === activeKind).map(asset => ({ id: asset.id, label: asset.name, image_url: asset.image_url || null, status: asset.image_url ? '已就绪' : asset.status === '生成中' ? '生成中 · 暂无图片' : '缺少图片' })); const renderOptions = () => { const options = getOptions(); nodeById.clear(); for (const option of options) { nodeById.set(option.id, option.id === '__placeholder__' ? { type: 'reference', asset_id: `placeholder-${Date.now()}`, asset_type: 'placeholder', label: '占位图', image_url: null } : { type: 'reference', asset_id: option.id, asset_type: activeKind, label: option.label, image_url: option.image_url }); } body.innerHTML = options.length ? `<div class="drama-reference-picker-grid">${options.map(option => { const alreadyAdded = existing.has(option.id); const checked = selected.has(option.id) || alreadyAdded; const image = option.image_url ? `<img src="${escapeHtml(resolveMediaUrl(option.image_url))}" alt="" />` : '<span class="drama-reference-picker-placeholder">＋</span>'; const missing = !option.image_url; return `<button type="button" class="drama-reference-option ${checked ? 'selected' : ''} ${alreadyAdded ? 'already-added' : ''} ${missing ? 'missing' : ''}" data-reference-option="${escapeHtml(option.id)}" aria-pressed="${checked}" ${alreadyAdded ? 'aria-label="已添加"' : ''}><span class="drama-reference-option-image">${image}</span><span class="drama-reference-option-info"><b>${escapeHtml(option.label)}</b><small>${alreadyAdded ? '已添加' : option.status}</small></span><span class="drama-reference-option-check">${checked ? '✓' : ''}</span></button>`; }).join('')}</div>` : `<div class="drama-reference-picker-empty"><div>♧</div><p>暂无${activeKind === 'character' ? '角色' : activeKind === 'scene' ? '场景' : activeKind === 'prop' ? '道具' : '占位图'}素材。</p><small>未生成图片的素材也可以先添加，生成视频前需要先补齐图片。</small></div>`; body.querySelectorAll<HTMLElement>('[data-reference-option]').forEach(option => option.addEventListener('click', () => { const id = option.dataset.referenceOption || ''; if (!id || existing.has(id)) return; if (selected.has(id)) selected.delete(id); else selected.add(id); renderOptions(); updateCount(); })); updateCount(); }; const updateCount = () => { const count = modal.querySelector<HTMLElement>('.drama-reference-picker-count'); if (count) count.textContent = `已选择 ${selected.size} 项`; }; const close = () => modal.remove(); modal.querySelectorAll<HTMLElement>('.close,[data-reference-cancel]').forEach(button => button.addEventListener('click', close)); modal.querySelectorAll<HTMLElement>('[data-reference-kind]').forEach(button => button.addEventListener('click', () => { activeKind = button.dataset.referenceKind as PickerKind; modal.querySelectorAll('[data-reference-kind]').forEach(item => item.classList.toggle('active', item === button)); renderOptions(); })); modal.querySelector('[data-reference-complete]')?.addEventListener('click', () => { const nodes = [...selected].map(id => nodeById.get(id)).filter((node): node is DramaPromptNode => Boolean(node)); close(); if (nodes.length) onComplete(nodes); }); modal.addEventListener('click', event => { if (event.target === modal) close(); }); picker.addEventListener('click', event => event.stopPropagation()); renderOptions(); }
function appendDramaReferenceNodes(project: ApiProject, nodes: DramaPromptNode[]) { const editor = document.querySelector<HTMLElement>('.drama-rich-prompt-editor'); if (!editor || !nodes.length) return; editor.focus(); const selection = window.getSelection(); const range = selection && selection.rangeCount > 0 && editor.contains(selection.getRangeAt(0).startContainer) ? selection.getRangeAt(0).cloneRange() : document.createRange(); if (!selection || range.collapsed === false || !editor.contains(range.startContainer)) { range.selectNodeContents(editor); range.collapse(false); } for (const node of nodes) { const temporary = document.createElement('span'); renderDramaPromptNodes(temporary, project, [node]); const chip = temporary.firstElementChild; if (!chip) continue; range.insertNode(chip); const spacer = document.createTextNode(' '); range.setStartAfter(chip); range.collapse(true); range.insertNode(spacer); range.setStartAfter(spacer); range.collapse(true); } selection?.removeAllRanges(); selection?.addRange(range); editor.dispatchEvent(new Event('input', { bubbles: true })); }
document.addEventListener('click', event => { const target = event.target instanceof HTMLElement ? event.target : null; const button = target?.closest<HTMLElement>('[data-drama-add-reference]'); if (!button || !dramaViewState.projectId) return; event.preventDefault(); event.stopPropagation(); const editor = document.querySelector<HTMLElement>('.drama-rich-prompt-editor'); if (!editor) { toast('分镜富文本编辑器尚未准备好'); return; } void fetch(`${API_BASE_URL}/projects/${dramaViewState.projectId}`).then(response => { if (!response.ok) throw new Error(`HTTP ${response.status}`); return response.json() as Promise<ApiProject>; }).then(project => openDramaReferencePicker(project, readDramaPromptNodes(editor), nodes => { appendDramaReferenceNodes(project, nodes); toast(`已添加 ${nodes.length} 项参考图，请保存分镜修改`); })).catch(error => { toast('参考图加载失败'); console.error(error); }); }, true);
document.addEventListener('click', event => {
  const target = event.target instanceof HTMLElement ? event.target : null;
  const qualityButton = target?.closest<HTMLButtonElement>('[data-drama-quality-check]');
  const autoMatchButton = target?.closest<HTMLButtonElement>('[data-drama-auto-match]');
  if ((!qualityButton && !autoMatchButton) || !dramaViewState.projectId) return;
  event.preventDefault();
  event.stopPropagation();
  const projectId = dramaViewState.projectId;
  const path = qualityButton
    ? `/projects/${projectId}/shots/${dramaViewState.shotId || ''}/quality`
    : `/projects/${projectId}/shots/${dramaViewState.shotId || ''}/auto-match-references`;
  const button = qualityButton || autoMatchButton;
  if (!button || !dramaViewState.shotId) return;
  button.disabled = true;
  button.textContent = qualityButton ? '⟳ 检查任务已创建' : '⟳ 匹配任务已创建';
  void fetch(`${API_BASE_URL}${path}`, { method: 'POST' })
    .then(async response => {
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      return response.json() as Promise<GenerationTask>;
    })
    .then(() => {
      toast(qualityButton ? '分镜质量检查任务已创建' : '参考图自动匹配任务已创建');
      void loadDramaDetail(projectId);
    })
    .catch(error => {
      button.disabled = false;
      button.textContent = qualityButton ? '运行检查' : '自动匹配参考图';
      toast(qualityButton ? '质量检查任务创建失败' : '自动匹配任务创建失败');
      console.error(error);
    });
}, true);
window.addEventListener('popstate', () => applyRoute(parseRoute()));
applyRoute(parseRoute(), true);
void loadModelSettings();
document.addEventListener('click', event => {
  const target = event.target instanceof HTMLElement ? event.target.closest<HTMLElement>('[data-drama-collapse-assets]') : null;
  if (!target) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  const sheet = target.closest<HTMLElement>('.drama-asset-sheet');
  const list = sheet?.querySelector<HTMLElement>('.drama-sheet-list');
  if (!list) return;
  const collapsed = !list.classList.contains('items-collapsed');
  list.classList.toggle('items-collapsed', collapsed);
  target.textContent = collapsed ? '▤ 展开' : '▤ 收起';
}, true);
