/** Drama dialogs and DOM observers: keeps modal behavior separate from the core editor renderer. */
import type { ApiProject, DramaAssetKind, ModelKind, ModelSettings } from './models.js';
import * as core from './drama_core_ui.js';
import * as shotUi from './drama_shot_ui.js';
import { syncDramaDecompositionBanner } from './drama_decomposition_banner_ui.js';
import { activeDramaProject, dramaViewState } from './drama_state.js';
import { icon } from './ui_icons.js';
import { openDramaExpandedScriptModal } from './drama_expanded_script_ui.js';
import { syncDramaVideoHistoryActions } from './drama_video_history_actions_ui.js';
import { configureDramaVideoCancellation } from './drama_video_cancellation_ui.js';
import { syncDramaVideoBatchGeneration } from './drama_video_batch_generation_ui.js';
import { apiKeyVisibilityIcon } from './settings_ui.js';
import { flushDramaEditorAutosave } from './drama_editor_autosave.js';

export type StorageSettingsResponse = { provider: 'local' | 'tos' | 'cos' | 'oss'; endpoint?: string; bucket?: string; region?: string; prefix?: string; public_base_url?: string; secret_id?: string; secret_key?: string; secret_id_set?: boolean; secret_key_set?: boolean };
type StorageField = (HTMLInputElement | HTMLSelectElement) & { placeholder?: string };

type DramaModalRuntime = {
  apiBaseUrl: string;
  projects: { id: string; name: string; status: string; ratio: string; style: string; theme: string; createdAt: string; scenes: number; characters: number; locations: number; props: number }[];
  projectFromApi: (project: ApiProject) => DramaModalRuntime['projects'][number];
  render: () => void;
  escapeHtml: (value: unknown) => string;
  toast: (message: string) => void;
  loadDramaDetail: (id: string, retry?: number) => Promise<void>;
  loadDramaProjects: () => Promise<void>;
  loadModelSettings: () => Promise<boolean>;
  applyModelSelect: (root: HTMLElement, selector: string, kind: ModelKind, selected?: string) => void;
  deleteDramaProject: (projectId: string, fromDetail?: boolean) => Promise<void>;
};

let runtime: DramaModalRuntime;
const rt = () => runtime;
export function configureDramaModalRuntime(value: DramaModalRuntime) { runtime = value; configureDramaVideoCancellation({ apiBaseUrl: value.apiBaseUrl, getActiveProject: () => activeDramaProject, getSelectedShot: core.dramaSelectedShot, getVideoTask: (project, shotId) => core.activeDramaTask(project, 'shot_video', shotId) || core.latestDramaTask(project, 'shot_video', shotId), toast: value.toast, reloadProject: value.loadDramaDetail }); }
const app = document.querySelector<HTMLDivElement>('#app')!;

document.addEventListener('click', async event => {
  const target = event.target instanceof HTMLElement ? event.target.closest<HTMLElement>('[data-drama-shot]') : null;
  if (!target || !activeDramaProject) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  try { await flushDramaEditorAutosave(); } catch { return; }
  dramaViewState.shotId = target.dataset.dramaShot || null;
  dramaViewState.videoUrl = null;
  void rt().loadDramaDetail(activeDramaProject.id);
}, true);

async function retryDramaScriptDecomposition(projectId: string) {
  try {
    const response = await fetch(`${rt().apiBaseUrl}/projects/${encodeURIComponent(projectId)}/script-decomposition/retry`, { method: 'POST' });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const task = await response.json() as { type?: string };
    rt().toast(task.type === 'script_expansion' ? '已重试剧本扩写' : '已重新生成故事圣经和剧本');
    await rt().loadDramaDetail(projectId);
    window.dispatchEvent(new CustomEvent('drama-screenplay-task-queued', { detail: projectId }));
  } catch (error) {
    rt().toast(`剧本重试失败：${error instanceof Error ? error.message : '请稍后重试'}`);
    console.error(error);
  }
}

function alignDramaAssetSheetToWorkspace() {
  const detail = document.querySelector<HTMLElement>('.drama-detail');
  const main = detail?.closest<HTMLElement>('.shell > main');
  if (!detail || !main) return;
  const bounds = main.getBoundingClientRect();
  detail.style.setProperty('--drama-workspace-left', `${Math.max(0, bounds.left)}px`);
  detail.style.setProperty('--drama-workspace-right', `${Math.max(0, window.innerWidth - bounds.right)}px`);
}

export function bindDramaWorkspace(project: ApiProject) { core.bindDramaAssetDrawer(project); alignDramaAssetSheetToWorkspace(); syncDramaVideoBatchGeneration({ apiBaseUrl: rt().apiBaseUrl, project, reloadProject: rt().loadDramaDetail, resolveMediaUrl: core.resolveMediaUrl, toast: rt().toast }); document.querySelector('#drama-back')?.addEventListener('click', () => { dramaViewState.projectId = null; dramaViewState.shotId = null; dramaViewState.assetPanel = null; rt().render(); }); document.querySelectorAll<HTMLElement>('[data-drama-shot]').forEach(element => element.addEventListener('click', () => { dramaViewState.shotId = element.dataset.dramaShot || null; dramaViewState.videoUrl = null; void rt().loadDramaDetail(project.id); })); document.querySelectorAll<HTMLElement>('[data-drama-add-shot]').forEach(element => element.addEventListener('click', event => { event.preventDefault(); event.stopPropagation(); void shotUi.addDramaShot(project.id, element.dataset.dramaAddShot || ''); })); document.querySelectorAll<HTMLElement>('[data-drama-delete-shot]').forEach(element => element.addEventListener('click', event => { event.preventDefault(); event.stopPropagation(); void shotUi.deleteDramaShot(project.id, element.dataset.dramaDeleteShot || ''); })); document.querySelectorAll<HTMLElement>('[data-drama-open-assets]').forEach(element => element.addEventListener('click', () => { dramaViewState.assetPanel = (element.dataset.dramaOpenAssets || 'character') as DramaAssetKind; const main = document.querySelector('main'); if (main) { main.innerHTML = core.dramaDetailMarkup(project); bindDramaWorkspace(project); } })); document.querySelectorAll<HTMLElement>('[data-drama-asset-tab]').forEach(element => element.addEventListener('click', () => { dramaViewState.assetPanel = element.dataset.dramaAssetTab as DramaAssetKind; const main = document.querySelector('main'); if (main) { main.innerHTML = core.dramaDetailMarkup(project); bindDramaWorkspace(project); } })); document.querySelectorAll<HTMLElement>('[data-drama-close-sheet]').forEach(element => element.addEventListener('click', () => { dramaViewState.assetPanel = null; const main = document.querySelector('main'); if (main) { main.innerHTML = core.dramaDetailMarkup(project); bindDramaWorkspace(project); } })); document.querySelectorAll<HTMLElement>('[data-drama-history-url]').forEach(element => element.addEventListener('click', () => { dramaViewState.videoUrl = element.dataset.dramaHistoryUrl || null; const main = document.querySelector('main'); if (main) { main.innerHTML = core.dramaDetailMarkup(project); bindDramaWorkspace(project); } })); document.querySelectorAll<HTMLElement>('[data-drama-generate-asset]').forEach(element => element.addEventListener('click', () => void core.dramaRunTask(`/projects/${project.id}/assets/${element.dataset.dramaGenerateAsset}/image`, '素材图片任务已创建'))); document.querySelectorAll<HTMLElement>('[data-drama-edit-asset]').forEach(element => element.addEventListener('click', async () => { const asset = core.dramaAssets(project).find(item => item.id === element.dataset.dramaEditAsset); if (!asset) return; const name = window.prompt('素材名称', asset.name); if (name === null) return; const prompt = window.prompt('素材提示词', asset.prompt); if (prompt === null) return; const response = await fetch(`${rt().apiBaseUrl}/projects/${project.id}/assets/${asset.id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, prompt }) }); if (response.ok) { rt().toast('素材已保存'); void rt().loadDramaDetail(project.id); } else rt().toast('素材保存失败'); })); document.querySelector('[data-drama-generate-all-assets]')?.addEventListener('click', () => { const assets = core.dramaAssets(project).filter(asset => asset.type === dramaViewState.assetPanel); void Promise.all(assets.map(asset => core.dramaRunTask(`/projects/${project.id}/assets/${asset.id}/image`, ''))).then(() => rt().toast(`已创建 ${assets.length} 个${core.dramaKindLabel(dramaViewState.assetPanel || 'prop')}图片任务`)); }); document.querySelector('[data-drama-refresh]')?.addEventListener('click', () => void rt().loadDramaDetail(project.id)); const shot = core.dramaSelectedShot(project); if (!shot) return; document.querySelector('#drama-save-shot')?.addEventListener('click', async () => { const response = await fetch(`${rt().apiBaseUrl}/projects/${project.id}/shots/${shot.id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ title: (document.querySelector('#drama-shot-title') as HTMLInputElement).value, original_text: (document.querySelector('#drama-shot-original') as HTMLTextAreaElement).value, prompt: (document.querySelector('#drama-shot-prompt') as HTMLTextAreaElement).value }) }); if (response.ok) { rt().toast('分镜修改已保存'); void rt().loadDramaDetail(project.id); } else rt().toast('分镜保存失败'); }); document.querySelector('#drama-generate-shot-prompt')?.addEventListener('click', () => void core.dramaRunTask(`/projects/${project.id}/shots/${shot.id}/prompt`, '分镜提示词任务已创建')); document.querySelector('#drama-generate-shot-video')?.addEventListener('click', () => void core.dramaRunTask(`/projects/${project.id}/shots/${shot.id}/video`, '分镜视频任务已创建')); document.querySelector('#drama-copy-shot-prompt')?.addEventListener('click', async () => { const text = (document.querySelector('#drama-shot-prompt') as HTMLTextAreaElement).value; await navigator.clipboard?.writeText(text); rt().toast('分镜提示词已复制'); }); document.querySelector('#drama-generate-all-prompts')?.addEventListener('click', () => { void Promise.all(core.dramaShots(project).map(item => core.dramaRunTask(`/projects/${project.id}/shots/${item.id}/prompt`, ''))).then(() => rt().toast('已创建全部分镜提示词任务')); }); document.querySelector('#drama-generate-all-videos')?.addEventListener('click', () => { void Promise.all(core.dramaShots(project).map(item => core.dramaRunTask(`/projects/${project.id}/shots/${item.id}/video`, ''))).then(() => rt().toast('已创建全部分镜视频任务')); }); }
export function openModernDramaModal() { const modal = document.createElement('div'); modal.className = 'modal-backdrop'; modal.id = 'drama-create-modal'; modal.innerHTML = `<div class="modal drama-create-modal"><button class="close" data-drama-modal-close>×</button><div class="modal-head"><div class="eyebrow">DRAMA PROJECT / NEW</div><h2>新建短剧</h2><p>先上传或粘贴剧本内容，再补充短剧的基础配置。项目会立即创建，分镜会在后台提取并回填；素材图片在提示词确认后手动生成。</p></div><div class="drama-create-stepper"><span class="active" data-step-label="1">1 文本来源</span><span data-step-label="2">2 基础配置</span></div><section id="drama-create-step-1"><h3>选择文本来源</h3><div class="drama-source-tabs"><label><input type="file" id="drama-file" accept=".txt,.md,.text" />⌃ 上传文件</label><button class="active" id="drama-paste-tab">▣ 粘贴文本</button></div><label>粘贴文本内容 <span id="drama-char-count">0 字</span><textarea id="drama-script" rows="11" placeholder="请将小说、剧本等文本内容粘贴到此处..."></textarea><div class="hint">剧本文本不少于 10 个字，创建后会异步拆解为分镜、角色、场景和道具。</div><div class="modal-actions"><button class="ghost" data-drama-modal-close>取消</button><button class="primary" id="drama-next">下一步 →</button></div></section><section id="drama-create-step-2" hidden><h3>短剧基础配置</h3><label>项目名称 <em>*</em><input id="drama-name" placeholder="建议使用书名 / 剧名 + 集数 / 部分命名" /></label><div class="form-grid"><label>基础视频比例<select id="drama-ratio"><option selected>9:16</option><option>16:9</option></select></label><label>视频风格<select id="drama-style"><option selected>真人风格</option><option>2D动漫风</option><option>3D动漫风</option></select></label><label>叙述背景主题<select id="drama-theme"><option selected>都市</option><option>悬疑</option><option>科幻</option><option>古风</option><option>玄幻</option></select></label><label>语言模型<select id="drama-language-model"><option selected>doubao-seed</option><option>gpt-4o-mini</option></select></label><label>图像 / 视频模型<select id="drama-multimodal-model"><option selected>doubao-seeddream</option><option>gpt-image-1</option></select></label></div><div class="modal-actions"><button class="ghost" id="drama-prev">← 上一步</button><button class="ghost" data-drama-modal-close>取消</button><button class="primary" id="drama-create">创建项目</button></div></section></div>`; document.body.append(modal); const close = () => modal.remove(); modal.querySelectorAll<HTMLElement>('[data-drama-modal-close]').forEach(element => element.addEventListener('click', close)); const script = modal.querySelector<HTMLTextAreaElement>('#drama-script')!; const count = modal.querySelector('#drama-char-count')!; script.addEventListener('input', () => { count.textContent = `${script.value.length} 字`; }); modal.querySelector<HTMLInputElement>('#drama-file')?.addEventListener('change', async event => { const file = (event.target as HTMLInputElement).files?.[0]; if (file) { script.value = await file.text(); script.dispatchEvent(new Event('input')); } }); modal.querySelector('#drama-next')?.addEventListener('click', () => { if (script.value.trim().length < 10) { rt().toast('剧本文本不少于 10 个字'); return; } (modal.querySelector('#drama-create-step-1') as HTMLElement).hidden = true; (modal.querySelector('#drama-create-step-2') as HTMLElement).hidden = false; modal.querySelector('[data-step-label="1"]')?.classList.remove('active'); modal.querySelector('[data-step-label="2"]')?.classList.add('active'); }); modal.querySelector('#drama-prev')?.addEventListener('click', () => { (modal.querySelector('#drama-create-step-1') as HTMLElement).hidden = false; (modal.querySelector('#drama-create-step-2') as HTMLElement).hidden = true; modal.querySelector('[data-step-label="1"]')?.classList.add('active'); modal.querySelector('[data-step-label="2"]')?.classList.remove('active'); }); modal.querySelector('#drama-create')?.addEventListener('click', async () => { const value = (id: string) => (modal.querySelector(`#${id}`) as HTMLInputElement | HTMLSelectElement).value; const name = value('drama-name').trim(); if (!name) { rt().toast('请填写项目名称'); return; } const button = modal.querySelector<HTMLButtonElement>('#drama-create')!; button.disabled = true; button.textContent = '创建中…'; try { const response = await fetch(`${rt().apiBaseUrl}/projects`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, script: script.value.trim(), ratio: value('drama-ratio'), style: value('drama-style'), theme: value('drama-theme'), language_model: value('drama-language-model'), multimodal_model: value('drama-multimodal-model') }) }); if (!response.ok) throw new Error(`HTTP ${response.status}`); const project = await response.json() as ApiProject; rt().projects.unshift(rt().projectFromApi(project)); close(); dramaViewState.shotId = null; dramaViewState.videoUrl = null; void rt().loadDramaDetail(project.id); } catch (error) { button.disabled = false; button.textContent = '创建项目'; rt().toast('创建失败，请确认后端已启动'); console.error(error); } }); }
export function openVideoPublicPromptModal(project: ApiProject) { const defaultPrompt = `整体保持${project.style || '真人风格'}，题材为${project.theme || '都市'}，按剧本处理方式组织镜头。\n视频全程保持画面内所有物体、道具、摆件数量不变，物体不消失、不凭空新增，物体位置轻微变化，物体形态材质保持一致，镜头平滑运动，无物体闪烁，无物体突然出现或突然消失，时序连贯，画面一致性强，流畅过渡`; const modal = document.createElement('div'); modal.className = 'modal-backdrop'; modal.innerHTML = `<div class="modal video-prompt-modal"><button class="close" aria-label="关闭">×</button><div class="modal-head"><h2>视频公共提示词</h2><p>设置分镜视频生成时统一追加的公共提示词。</p></div><div class="video-prompt-body"><textarea id="video-public-prompt-input" rows="4" autofocus>${rt().escapeHtml(project.video_public_prompt?.trim() || defaultPrompt)}</textarea></div><div class="video-prompt-actions"><button class="ghost" id="video-public-prompt-default">↶&nbsp; 恢复默认</button><button class="ghost" id="video-public-prompt-cancel">取消</button><button class="primary" id="video-public-prompt-save">保存</button></div></div>`; document.body.append(modal); const close = () => modal.remove(); modal.querySelectorAll<HTMLElement>('.close,#video-public-prompt-cancel').forEach(element => element.addEventListener('click', close)); modal.querySelector('#video-public-prompt-default')?.addEventListener('click', () => { const input = modal.querySelector<HTMLTextAreaElement>('#video-public-prompt-input'); if (input) input.value = defaultPrompt; }); modal.querySelector('#video-public-prompt-save')?.addEventListener('click', async () => { const input = modal.querySelector<HTMLTextAreaElement>('#video-public-prompt-input'); const button = modal.querySelector<HTMLButtonElement>('#video-public-prompt-save'); if (!input || !button) return; button.disabled = true; try { const response = await fetch(`${rt().apiBaseUrl}/projects/${project.id}/video-public-prompt`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ video_public_prompt: input.value }) }); if (!response.ok) throw new Error(`HTTP ${response.status}`); rt().toast('视频公共提示词已保存'); close(); void rt().loadDramaDetail(project.id); } catch (error) { button.disabled = false; rt().toast('视频公共提示词保存失败'); console.error(error); } }); modal.querySelector<HTMLTextAreaElement>('#video-public-prompt-input')?.focus(); }
export function ensureVideoPublicPromptButton() { const toolbar = document.querySelector<HTMLElement>('.drama-detail-toolbar'); if (toolbar && !toolbar.querySelector('#drama-open-video-public-prompt')) { const button = document.createElement('button'); button.className = 'ghost compact drama-video-prompt-button'; button.id = 'drama-open-video-public-prompt'; button.textContent = '视频公共提示词'; button.addEventListener('click', async () => { if (!dramaViewState.projectId) return; try { const response = await fetch(`${rt().apiBaseUrl}/projects/${dramaViewState.projectId}`); if (!response.ok) throw new Error(`HTTP ${response.status}`); openVideoPublicPromptModal(await response.json() as ApiProject); } catch (error) { rt().toast('视频公共提示词加载失败'); console.error(error); } }); toolbar.append(button); } const detail = document.querySelector<HTMLElement>('.drama-detail'); const workspace = detail?.querySelector<HTMLElement>('.drama-workspace-grid'); if (workspace && !workspace.querySelector('.drama-asset-rail')) { const rail = document.createElement('aside'); rail.className = 'drama-asset-rail'; rail.setAttribute('aria-label', '素材配置'); rail.innerHTML = (['character', 'scene', 'prop'] as DramaAssetKind[]).map(kind => `<button type="button" class="drama-asset-rail-item" data-drama-rail-kind="${kind}" title="打开${core.dramaKindLabel(kind)}配置"><span class="drama-asset-rail-icon">${kind === 'character' ? '♙' : kind === 'scene' ? '▦' : '◇'}</span><span>${core.dramaKindLabel(kind)}</span></button>`).join(''); rail.querySelectorAll<HTMLElement>('[data-drama-rail-kind]').forEach(button => button.addEventListener('click', () => { const kind = button.dataset.dramaRailKind as DramaAssetKind; dramaViewState.assetPanel = kind; if (dramaViewState.projectId) void rt().loadDramaDetail(dramaViewState.projectId); })); workspace.append(rail); }}
export function ensureDramaPlaceholderRailItem() {
  const detail = document.querySelector<HTMLElement>('.drama-detail');
  const rail = detail?.querySelector<HTMLElement>('.drama-asset-rail');
  if (!rail || rail.querySelector('[data-drama-placeholder-rail]')) return;
  if (!rail.querySelector('[data-drama-frame-rail]')) {
    const frameButton = document.createElement('button');
    frameButton.type = 'button';
    frameButton.className = 'drama-asset-rail-item';
    frameButton.dataset.dramaFrameRail = 'true';
    frameButton.title = '设置首尾帧';
    frameButton.innerHTML = '<span class="drama-asset-rail-icon">▣</span><span>首尾帧</span>';
    frameButton.addEventListener('click', () => { if (activeDramaProject) core.openDramaFrameModal(activeDramaProject); });
    rail.append(frameButton);
  }
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'drama-asset-rail-item drama-placeholder-rail-item';
  button.dataset.dramaPlaceholderRail = 'true';
  button.title = '打开占位图配置';
  button.innerHTML = '<span class="drama-asset-rail-icon">▱</span><span>占位图</span>';
  button.addEventListener('click', async event => {
    event.preventDefault();
    event.stopPropagation();
    if (activeDramaProject) {
      core.openDramaPlaceholderModal(activeDramaProject);
      return;
    }
    if (!dramaViewState.projectId) return;
    try {
      const response = await fetch(`${rt().apiBaseUrl}/projects/${dramaViewState.projectId}`);
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      core.openDramaPlaceholderModal(await response.json() as ApiProject);
    } catch (error) {
      rt().toast('占位图配置加载失败');
      console.error(error);
    }
  });
  rail.append(button);
}

export function openDramaModelSelectionModal(project: ApiProject) {
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop';
  modal.innerHTML = `<div class="modal video-prompt-modal model-selection-modal"><button class="close" aria-label="关闭">×</button><div class="modal-head"><h2>短剧模型配置</h2><p>修改当前短剧使用的模型，不会影响设置页的 endpoint 和其他项目。</p></div><label>语言模型<select id="drama-model-language"></select></label><label>图像模型<select id="drama-model-multimodal"></select></label><label>视频模型<select id="drama-model-video"></select></label><div class="video-prompt-actions"><button class="ghost" id="drama-model-cancel">取消</button><button class="primary" id="drama-model-save">保存</button></div></div>`;
  document.body.append(modal);
  rt().applyModelSelect(modal, '#drama-model-language', 'language', project.language_model || '');
  rt().applyModelSelect(modal, '#drama-model-multimodal', 'multimodal', project.multimodal_model || '');
  rt().applyModelSelect(modal, '#drama-model-video', 'video', project.video_model || '');
  const close = () => modal.remove();
  modal.querySelectorAll<HTMLElement>('.close,#drama-model-cancel').forEach(element => element.addEventListener('click', close));
  modal.querySelector('#drama-model-save')?.addEventListener('click', async () => {
    const button = modal.querySelector<HTMLButtonElement>('#drama-model-save');
    const payload = { language_model: modal.querySelector<HTMLSelectElement>('#drama-model-language')?.value || '', multimodal_model: modal.querySelector<HTMLSelectElement>('#drama-model-multimodal')?.value || '', video_model: modal.querySelector<HTMLSelectElement>('#drama-model-video')?.value || '' };
    if (button) { button.disabled = true; button.textContent = '保存中…'; }
    try {
      const response = await fetch(`${rt().apiBaseUrl}/projects/${project.id}/models`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload) });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      rt().toast('短剧模型配置已保存'); close(); void rt().loadDramaDetail(project.id);
    } catch (error) { if (button) { button.disabled = false; button.textContent = '保存'; } rt().toast('短剧模型配置保存失败'); console.error(error); }
  });
}

export async function openDramaGlobalParamsModal(project: ApiProject) {
  await rt().loadModelSettings();
  const constraints = project.shot_constraints || {};
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop';
  modal.innerHTML = `<div class="modal video-prompt-modal drama-global-params-modal"><button class="close" aria-label="关闭">×</button><div class="modal-head"><h2>全局参数</h2><p>保存当前项目的生成配置；不会自动生成或重新生成任何分镜内容。</p></div><div class="drama-global-params-form"><label>生成视频的比例<select id="global-ratio"><option ${project.ratio === '9:16' ? 'selected' : ''}>9:16</option><option ${project.ratio === '16:9' ? 'selected' : ''}>16:9</option></select></label><label>视频风格<select id="global-style"><option ${project.style === '真人风格' ? 'selected' : ''}>真人风格</option><option ${project.style === '2D动漫风' ? 'selected' : ''}>2D动漫风</option><option ${project.style === '3D动漫风' ? 'selected' : ''}>3D动漫风</option></select></label><label>叙述背景主题<input id="global-theme" value="${rt().escapeHtml(project.theme || '都市')}" /></label><label>语言模型<select id="global-language-model"></select></label><label>图像模型<select id="global-multimodal-model"></select></label><label>视频模型<select id="global-video-model"></select></label><label>短剧分辨率<select id="global-resolution"><option ${project.resolution === '720p' ? 'selected' : ''}>720p</option><option ${project.resolution === '480p' ? 'selected' : ''}>480p</option></select></label><label>字幕<select id="global-subtitles"><option value="false" ${!constraints.subtitles ? 'selected' : ''}>不要字幕</option><option value="true" ${constraints.subtitles ? 'selected' : ''}>需要字幕</option></select></label><label>背景音乐<select id="global-background-music"><option value="false" ${!constraints.background_music ? 'selected' : ''}>不要背景音乐</option><option value="true" ${constraints.background_music ? 'selected' : ''}>需要背景音乐</option></select></label></div><div class="video-prompt-actions"><button class="ghost" id="drama-global-params-cancel">取消</button><button class="primary" id="drama-global-params-save">保存</button></div></div>`;
  document.body.append(modal);
  rt().applyModelSelect(modal, '#global-language-model', 'language', project.language_model || '');
  rt().applyModelSelect(modal, '#global-multimodal-model', 'multimodal', project.multimodal_model || '');
  rt().applyModelSelect(modal, '#global-video-model', 'video', project.video_model || '');
  const close = () => modal.remove();
  modal.querySelector('.close')?.addEventListener('click', close);
  modal.querySelector('#drama-global-params-cancel')?.addEventListener('click', close);
  modal.querySelector('#drama-global-params-save')?.addEventListener('click', async () => {
    const button = modal.querySelector<HTMLButtonElement>('#drama-global-params-save');
    const value = (selector: string) => (modal.querySelector<HTMLInputElement | HTMLSelectElement>(selector)?.value || '').trim();
    const parameterPayload = {
      ratio: value('#global-ratio'),
      style: value('#global-style'),
      theme: value('#global-theme'),
      resolution: value('#global-resolution'),
      shot_constraints: { subtitles: value('#global-subtitles') === 'true', background_music: value('#global-background-music') === 'true' },
    };
    const modelPayload = {
      language_model: value('#global-language-model'),
      multimodal_model: value('#global-multimodal-model'),
      video_model: value('#global-video-model'),
    };
    if (button) { button.disabled = true; button.textContent = '保存中…'; }
    try {
      const modelResponse = await fetch(`${rt().apiBaseUrl}/projects/${project.id}/models`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(modelPayload) });
      if (!modelResponse.ok) throw new Error(`HTTP ${modelResponse.status}`);
      const parameterResponse = await fetch(`${rt().apiBaseUrl}/projects/${project.id}/parameters`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(parameterPayload) });
      if (!parameterResponse.ok) throw new Error(`HTTP ${parameterResponse.status}`);
      rt().toast('全局参数和模型配置已保存');
      close();
      void rt().loadDramaDetail(project.id);
    } catch (error) {
      if (button) { button.disabled = false; button.textContent = '保存'; }
      rt().toast('全局参数保存失败');
      console.error(error);
    }
  });
}

export function ensureDramaDetailToolbar() {
  document.querySelector('#drama-save-shot')?.remove();
  const toolbar = document.querySelector<HTMLElement>('.drama-detail-toolbar');
  if (!toolbar) return;
  syncDramaDecompositionBanner(activeDramaProject, projectId => void retryDramaScriptDecomposition(projectId));
  if (toolbar.querySelector('[data-drama-top-actionbar]')) return;
  ensureVideoPublicPromptButton();
  toolbar.querySelectorAll('.drama-config-field').forEach(field => field.remove());
  const detail = toolbar.closest<HTMLElement>('.drama-detail');
  const legacyActions = detail?.querySelector<HTMLElement>('.drama-detail-actions');
  const actionbar = document.createElement('div');
  actionbar.className = 'drama-top-actions';
  actionbar.dataset.dramaTopActionbar = 'true';
  const videoPromptButton = toolbar.querySelector<HTMLElement>('#drama-open-video-public-prompt');
  const batchPromptButton = legacyActions?.querySelector<HTMLElement>('#drama-generate-all-prompts');
  const allVideoButton = legacyActions?.querySelector<HTMLElement>('#drama-generate-all-videos');
  const cancelAllVideoButton = legacyActions?.querySelector<HTMLElement>('#drama-cancel-all-videos');
  const expandedScriptButton = document.createElement('button');
  expandedScriptButton.type = 'button';
  expandedScriptButton.className = 'ghost';
  expandedScriptButton.textContent = '剧本';
  expandedScriptButton.addEventListener('click', () => {
    if (dramaViewState.projectId) {
      openDramaExpandedScriptModal({
        apiBaseUrl: rt().apiBaseUrl,
        projectId: dramaViewState.projectId,
        toast: rt().toast,
        onScreenplayTaskQueued: () => rt().loadDramaDetail(dramaViewState.projectId!),
      });
    }
  });
  actionbar.append(expandedScriptButton);
  if (videoPromptButton) {
    videoPromptButton.classList.remove('compact');
    actionbar.append(videoPromptButton);
  }
  if (batchPromptButton) actionbar.append(batchPromptButton);
  if (allVideoButton) actionbar.append(allVideoButton);
  if (cancelAllVideoButton) actionbar.append(cancelAllVideoButton);
  legacyActions?.remove();
  const divider = document.createElement('span');
  divider.className = 'drama-toolbar-divider';
  divider.setAttribute('aria-hidden', 'true');
  actionbar.append(divider);
  const globalParamsButton = document.createElement('button');
  globalParamsButton.type = 'button';
  globalParamsButton.className = 'ghost';
  globalParamsButton.textContent = '☷  全局参数';
  globalParamsButton.addEventListener('click', async () => {
    if (!dramaViewState.projectId) return;
    try {
      const response = await fetch(`${rt().apiBaseUrl}/projects/${dramaViewState.projectId}`);
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      await openDramaGlobalParamsModal(await response.json() as ApiProject);
    } catch (error) { rt().toast('全局参数加载失败'); console.error(error); }
  });
  const previewButton = document.createElement('button');
  previewButton.type = 'button';
  previewButton.className = 'ghost drama-toolbar-preview';
  previewButton.textContent = '▷  预览';
  previewButton.disabled = true;
  previewButton.title = '视频生成后开放预览';
  const saveButton = document.createElement('button');
  saveButton.type = 'button';
  saveButton.className = 'primary';
  saveButton.textContent = '▣  保存';
  saveButton.addEventListener('click', async () => {
    const nameInput = document.querySelector<HTMLInputElement>('#drama-project-name');
    if (!nameInput?.value.trim()) {
      rt().toast('项目标题不能为空');
      nameInput?.focus();
      return;
    }
    saveButton.disabled = true;
    saveButton.textContent = '保存中…';
    try {
      await flushDramaEditorAutosave();
      rt().toast('项目修改已保存');
    } catch (error) {
      rt().toast('项目保存失败');
      console.error(error);
    } finally {
      saveButton.disabled = false;
      saveButton.textContent = '▣  保存';
    }
  });
  actionbar.append(globalParamsButton, previewButton, saveButton);
  toolbar.append(actionbar);
}

/** Render the prompt-version selector with the editor's compact dropdown treatment. */
function syncPromptVersionDropdown(select: HTMLSelectElement) {
  const dropdown = select.closest<HTMLElement>('.drama-prompt-panel')?.querySelector<HTMLElement>('[data-prompt-version-dropdown]');
  const trigger = dropdown?.querySelector<HTMLButtonElement>('[data-prompt-version-trigger]');
  if (!dropdown || !trigger) return;
  const option = select.selectedOptions.item(0);
  const label = trigger.querySelector<HTMLElement>('span')!;
  const text = option?.textContent || '选择镜头';
  if (label.textContent !== text) label.textContent = text;
  trigger.disabled = select.disabled;
  dropdown.querySelectorAll<HTMLElement>('[data-prompt-version-option]').forEach(item => {
    const active = item.dataset.promptVersionOption === select.value;
    item.classList.toggle('active', active);
    item.setAttribute('aria-selected', String(active));
  });
}

function ensurePromptVersionDropdown(select: HTMLSelectElement) {
  const panel = select.closest<HTMLElement>('.drama-prompt-panel'); if (panel?.querySelector('[data-prompt-version-dropdown]')) {
    syncPromptVersionDropdown(select);
    return;
  }
  select.hidden = true;
  const dropdown = document.createElement('span');
  dropdown.className = 'drama-prompt-version-dropdown';
  dropdown.dataset.promptVersionDropdown = 'true';
  const trigger = document.createElement('button');
  trigger.type = 'button';
  trigger.className = 'drama-prompt-version-trigger';
  trigger.dataset.promptVersionTrigger = 'true';
  trigger.setAttribute('aria-expanded', 'false');
  trigger.setAttribute('aria-haspopup', 'listbox');
  trigger.innerHTML = '<span></span><i aria-hidden="true">⌄</i>';
  const menu = document.createElement('span');
  menu.className = 'drama-prompt-version-menu';
  menu.setAttribute('role', 'listbox');
  menu.hidden = true;
  [...select.options].forEach(option => {
    const item = document.createElement('button');
    item.type = 'button';
    item.className = 'drama-prompt-version-option';
    item.dataset.promptVersionOption = option.value;
    item.setAttribute('role', 'option');
    item.textContent = option.textContent;
    item.addEventListener('click', () => {
      if (select.disabled || select.value === option.value) return;
      select.value = option.value;
      select.dispatchEvent(new Event('change', { bubbles: true }));
      menu.hidden = true;
      trigger.setAttribute('aria-expanded', 'false');
    });
    menu.append(item);
  });
  trigger.addEventListener('click', () => {
    if (trigger.disabled) return;
    const expanded = menu.hidden;
    menu.hidden = !expanded;
    trigger.setAttribute('aria-expanded', String(expanded));
  });
  dropdown.addEventListener('keydown', event => {
    if (event.key !== 'Escape') return;
    menu.hidden = true;
    trigger.setAttribute('aria-expanded', 'false');
    trigger.focus();
  });
  dropdown.addEventListener('focusout', () => window.setTimeout(() => {
    if (dropdown.contains(document.activeElement)) return;
    menu.hidden = true;
    trigger.setAttribute('aria-expanded', 'false');
  }));
  select.after(dropdown);
  dropdown.append(trigger, menu);
  panel?.querySelector<HTMLElement>('.section-title > div:last-child')?.prepend(dropdown);
  select.addEventListener('change', () => syncPromptVersionDropdown(select));
  syncPromptVersionDropdown(select);
}

/**
 * Turns the legacy prompt-version select into the two persisted generation
 * modes. The editor calls this after a shot is rendered so re-opening a shot
 * restores its selected mode before the next prompt task is created.
 */
export function ensureDramaPromptVersionSelector() {
  const project = activeDramaProject;
  const shot = project ? core.dramaSelectedShot(project) : undefined;
  const select = document.querySelector<HTMLSelectElement>('.drama-prompt-version');
  if (!project || !shot || !select) return;
  if (select.options.length < 2) {
    select.innerHTML = '<option value="v1">v1 · 多镜头</option><option value="v2">v2 · 长镜头</option>';
  }
  const selectedVersion = shot.prompt_template_version === 'v2' ? 'v2' : 'v1';
  if (!select.dataset.promptVersionBound) select.value = selectedVersion;
  ensurePromptVersionDropdown(select);
  if (select.dataset.promptVersionBound) return;
  select.dataset.promptVersionBound = 'true';
  select.addEventListener('change', async () => {
    const version = select.value;
    const promptButton = document.querySelector<HTMLButtonElement>('#drama-generate-shot-prompt');
    select.disabled = true;
    if (promptButton) promptButton.disabled = true;
    try {
      const response = await fetch(`${rt().apiBaseUrl}/projects/${project.id}/shots/${shot.id}`, {
        method: 'PUT', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ prompt_template_version: version }),
      });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      shot.prompt_template_version = version;
      rt().toast(`${version === 'v2' ? '长镜头' : '多镜头'}模板已选择，下次生成提示词时生效`);
    } catch (error) {
      select.value = selectedVersion;
      rt().toast('提示词模板切换失败');
      console.error(error);
    } finally {
      select.disabled = false;
      syncPromptVersionDropdown(select);
      if (promptButton) promptButton.disabled = false;
    }
  });
}

const dramaDomObserver = new MutationObserver(() => {
  ensureVideoPublicPromptButton();
  ensureDramaPlaceholderRailItem();
});
dramaDomObserver.observe(app, { childList: true, subtree: true });
const dramaDetailToolbarObserver = new MutationObserver(ensureDramaDetailToolbar);
dramaDetailToolbarObserver.observe(app, { childList: true, subtree: true });
const dramaPromptVersionObserver = new MutationObserver(ensureDramaPromptVersionSelector);
dramaPromptVersionObserver.observe(app, { childList: true, subtree: true });
export function ensureDramaReferencePickerButton() { const quickAssets = document.querySelector<HTMLElement>('.drama-reference-panel .drama-quick-assets'); if (!quickAssets) return; quickAssets.querySelectorAll('[data-drama-open-assets]').forEach(button => button.remove()); if (quickAssets.querySelector('[data-drama-add-reference]')) return; const description = document.querySelector<HTMLElement>('.drama-reference-panel .section-title p'); if (description) description.textContent = '添加已生成的角色、场景、道具图片或占位图，写入当前分镜富文本提示词。'; const button = document.createElement('button'); button.type = 'button'; button.className = 'ghost compact'; button.dataset.dramaAddReference = 'true'; button.textContent = '＋ 添加参考图'; quickAssets.prepend(button); }
const dramaReferencePickerButtonObserver = new MutationObserver(ensureDramaReferencePickerButton);
dramaReferencePickerButtonObserver.observe(app, { childList: true, subtree: true });
const dramaReferenceDisplayObserver = new MutationObserver(() => { if (activeDramaProject) core.syncDramaShotReferencePanel(activeDramaProject); });
dramaReferenceDisplayObserver.observe(app, { childList: true, subtree: true });
export function normalizeDramaRichPromptToolbar() { document.querySelectorAll('.drama-rich-prompt-toolbar').forEach(toolbar => toolbar.remove()); document.querySelectorAll<HTMLTextAreaElement>('.drama-prompt-panel textarea#drama-shot-prompt[hidden]').forEach(textarea => { textarea.classList.add('drama-rich-prompt-source'); textarea.style.display = 'none'; }); }
const dramaRichPromptToolbarObserver = new MutationObserver(normalizeDramaRichPromptToolbar);
dramaRichPromptToolbarObserver.observe(app, { childList: true, subtree: true });
export function storageField(id: string) { return document.querySelector<StorageField>(`#${id}`); }
export function storageProviderStatus(provider: StorageSettingsResponse['provider']) { if (provider === 'local') return '本地文件会保存到桌面应用数据目录'; return `${({ tos: '火山 TOS', cos: '腾讯 COS', oss: '阿里云 OSS' } as const)[provider]} 已启用`; }
export function storageEndpointPlaceholder(provider: StorageSettingsResponse['provider']) { return ({ local: '', tos: 'tos-cn-beijing.volces.com', cos: 'cos.ap-chengdu.myqcloud.com', oss: 'oss-cn-hangzhou.aliyuncs.com' } as const)[provider]; }
export function setStorageField(id: string, value: string) { const field = storageField(id); if (field) field.value = value; if (id === 'storage-provider') { const provider = value as StorageSettingsResponse['provider']; const status = document.querySelector<HTMLElement>('#storage-settings-status'); if (status) status.textContent = storageProviderStatus(provider); const endpoint = storageField('storage-endpoint'); if (endpoint) endpoint.placeholder = storageEndpointPlaceholder(provider); } }
export async function loadStorageSettings() { try { const response = await fetch(`${rt().apiBaseUrl}/settings/storage`); if (!response.ok) throw new Error(`HTTP ${response.status}`); const settings = await response.json() as StorageSettingsResponse; setStorageField('storage-provider', settings.provider || 'local'); setStorageField('storage-endpoint', settings.endpoint || ''); setStorageField('storage-bucket', settings.bucket || ''); setStorageField('storage-region', settings.region || ''); setStorageField('storage-prefix', settings.prefix || 'media'); setStorageField('storage-public-base-url', settings.public_base_url || ''); [['secret_id', settings.secret_id || '', 'SecretId'], ['secret_key', settings.secret_key || '', 'SecretKey']].forEach(([field, value, label]) => { const input = storageField(`storage-${field.replace('_', '-')}`) as HTMLInputElement | null; const button = document.querySelector<HTMLButtonElement>(`[data-storage-secret-toggle="${field}"]`); if (input) { input.value = value; input.type = 'text'; input.dataset.revealed = 'true'; input.placeholder = `请输入 ${label}`; } if (button) { button.innerHTML = apiKeyVisibilityIcon(true); button.title = `隐藏 ${label}`; button.setAttribute('aria-label', `隐藏 ${label}`); } }); } catch (error) { rt().toast('媒体存储配置加载失败'); console.error(error); } }
export async function toggleStorageCredential(field: 'secret_id' | 'secret_key') { const input = storageField(`storage-${field.replace('_', '-')}`) as HTMLInputElement | null; const button = document.querySelector<HTMLButtonElement>(`[data-storage-secret-toggle="${field}"]`); const label = field === 'secret_id' ? 'SecretId' : 'SecretKey'; if (!input || !button) return; const hidden = input.dataset.revealed === 'true'; input.type = hidden ? 'password' : 'text'; input.dataset.revealed = String(!hidden); button.innerHTML = apiKeyVisibilityIcon(!hidden); button.title = `${hidden ? '查看' : '隐藏'} ${label}`; button.setAttribute('aria-label', `${hidden ? '查看' : '隐藏'} ${label}`); }
export function ensureStorageSettingsCard() { const grid = document.querySelector<HTMLElement>('.settings-grid'); if (!grid || grid.querySelector('[data-storage-settings]')) return; const card = document.createElement('div'); card.className = 'settings-card storage-settings-card'; card.dataset.storageSettings = 'true'; card.innerHTML = `<div class="settings-card-header"><div class="setting-icon green">▣</div><div><h2>媒体存储</h2><p>视频和图片生成后保存到本地、火山 TOS、腾讯 COS 或阿里云 OSS。云存储保存前会嗅探上传和公开访问能力。</p></div></div><div class="storage-settings-form"><label>存储方式<select id="storage-provider"><option value="local">本地文件</option><option value="tos">火山引擎 TOS</option><option value="cos">腾讯云 COS</option><option value="oss">阿里云 OSS</option></select></label><label>Endpoint / 地址<input id="storage-endpoint" placeholder="https://oss-cn-hangzhou.aliyuncs.com" /></label><label>Bucket / 桶<input id="storage-bucket" placeholder="bucket-name" /></label><label>Region / 地域<input id="storage-region" placeholder="cn-hangzhou" /></label><label>SecretId / AccessKey ID<div class="storage-secret-input"><input id="storage-secret-id" autocomplete="off" placeholder="请输入 SecretId 或 AccessKey ID" /><button type="button" class="ghost storage-secret-toggle" data-storage-secret-toggle="secret_id" aria-label="隐藏 SecretId" title="隐藏 SecretId">${apiKeyVisibilityIcon(true)}</button></div></label><label>SecretKey / AccessKey Secret<div class="storage-secret-input"><input id="storage-secret-key" autocomplete="new-password" placeholder="请输入 SecretKey 或 AccessKey Secret" /><button type="button" class="ghost storage-secret-toggle" data-storage-secret-toggle="secret_key" aria-label="隐藏 SecretKey" title="隐藏 SecretKey">${apiKeyVisibilityIcon(true)}</button></div></label><label>对象前缀<input id="storage-prefix" value="media" placeholder="media" /></label><label class="storage-public-url">公开访问域名（可选）<input id="storage-public-base-url" placeholder="https://cdn.example.com/media" /></label></div><div class="storage-settings-footer"><span id="storage-settings-status">默认使用本地文件</span><button class="primary storage-save-button" id="save-storage-settings">嗅探上传并保存配置</button></div>`; grid.append(card); void loadStorageSettings(); }
const storageSettingsObserver = new MutationObserver(ensureStorageSettingsCard);
storageSettingsObserver.observe(app, { childList: true, subtree: true });
document.addEventListener('click', event => { const button = (event.target instanceof HTMLElement ? event.target : null)?.closest<HTMLButtonElement>('[data-storage-secret-toggle]'); const field = button?.dataset.storageSecretToggle; if (!button || (field !== 'secret_id' && field !== 'secret_key')) return; event.preventDefault(); void toggleStorageCredential(field); });
function ensureDramaVideoHistoryActions() { syncDramaVideoHistoryActions({ apiBaseUrl: rt().apiBaseUrl, project: activeDramaProject, resolveMediaUrl: core.resolveMediaUrl, loadDramaDetail: rt().loadDramaDetail, toast: rt().toast }); }
const dramaVideoDownloadObserver = new MutationObserver(ensureDramaVideoHistoryActions);
dramaVideoDownloadObserver.observe(app, { childList: true, subtree: true });
