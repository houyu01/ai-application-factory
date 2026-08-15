import * as core from './drama_core_ui.js';
import { syncDramaCoverUi } from './drama_cover_ui.js';
import { syncDramaDecompositionBanner } from './drama_decomposition_banner_ui.js';
import { activeDramaProject, dramaViewState, setActiveDramaProject } from './drama_state.js';
import { dramaAssetImageIsGenerating } from './drama_asset_image_state_ui.js';
import { dramaReferenceAsset } from './drama_reference_asset.js';
import { dramaShotVideoStatus } from './drama_video_history.js';
import { hasUnsavedDramaEditorChanges } from './drama_editor_autosave.js';
import { refreshDramaVideoBatchGeneration } from './drama_video_batch_generation_ui.js';
import { replaceDramaAssetDrawer } from './drama_asset_drawer_refresh.js';
import type { ApiProject, GenerationTask } from './models.js';

type PartialRefreshRuntime = {
  apiBaseUrl: string;
  loadFullDetail: (id: string) => Promise<void>;
  toast: (message: string) => void;
};

let runtime: PartialRefreshRuntime | null = null;

export function configureDramaPartialRefresh(value: PartialRefreshRuntime) {
  runtime = value;
}

function mergeTasks(project: ApiProject, tasks: GenerationTask[]) {
  const merged = new Map((project.tasks || []).map(task => [task.id, task]));
  const changedTypes = new Set(tasks.filter(task => JSON.stringify(merged.get(task.id)) !== JSON.stringify({ ...merged.get(task.id), ...task })).map(task => task.type));
  tasks.forEach(task => merged.set(task.id, { ...merged.get(task.id), ...task }));
  project.tasks = [...merged.values()];
  tasks.forEach(task => {
    if (task.type === 'asset_image' || task.type === 'placeholder_image' || task.type === 'cover_image') {
      const asset = project.assets?.find(item => item.id === task.resource_id);
      if (asset) asset.status = task.status;
    }
    if (task.type === 'asset_variant_image') {
      const variant = project.assets?.flatMap(asset => asset.variants || []).find(item => item.id === task.resource_id);
      if (variant) variant.status = task.status;
    }
    if (task.type === 'shot_video') {
      const shot = project.shots?.find(item => item.id === task.resource_id);
      if (shot) shot.status = task.status;
    }
  });
  setActiveDramaProject(project);
  core.applyDramaGenerationLoading(project, changedTypes);
  syncDramaPromptReferenceImages(project);
  syncDramaCoverUi(project);
  syncDramaDecompositionBanner(project, undefined, async (projectId, button) => {
    button.disabled = true; button.textContent = '停止中…';
    try {
      const response = await fetch(`${runtime!.apiBaseUrl}/projects/${projectId}/expanded-script/cancel`, { method: 'POST' });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      runtime!.toast('已按当前进度保存并停止生成');
      await runtime!.loadFullDetail(projectId);
    } catch (error) {
      button.disabled = false; button.textContent = '停止生成';
      runtime!.toast('停止生成失败，请稍后重试'); console.error(error);
    }
  });
}

/** Replace rich-prompt thumbnails only when their referenced asset crosses a loading boundary. */
function syncDramaPromptReferenceImages(project: ApiProject) {
  const editor = document.querySelector<HTMLElement>('.drama-rich-prompt-editor');
  if (!editor) return;
  const changed = [...editor.querySelectorAll<HTMLElement>('[data-drama-prompt-reference]')].some(chip => Boolean(chip.querySelector('.drama-image-loading')) !== dramaAssetImageIsGenerating(dramaReferenceAsset(project.assets || [], { asset_id: chip.dataset.assetId || '', variant_id: chip.dataset.variantId || null }), project.tasks));
  if (changed) core.renderDramaPromptNodes(editor, project, core.readDramaPromptNodes(editor));
}

function refreshShotList(project: ApiProject) {
  project.shots?.forEach(shot => {
    const nav = document.querySelector<HTMLElement>(`[data-drama-shot="${CSS.escape(shot.id)}"]`);
    const status = nav?.querySelector<HTMLElement>('.status');
    if (!status) return;
    const shotStatus = dramaShotVideoStatus(shot, core.dramaShotVideoTask(project, shot.id));
    status.className = `status ${core.dramaStatusClass(shotStatus)}`;
    status.textContent = core.dramaStatusText(shotStatus);
  });
}

function resetRichPromptEditor(project: ApiProject) {
  if (hasUnsavedDramaEditorChanges()) return;
  const panel = document.querySelector<HTMLElement>('.drama-prompt-panel');
  const textarea = panel?.querySelector<HTMLTextAreaElement>('#drama-shot-prompt');
  const shot = core.dramaSelectedShot(project);
  if (!panel || !textarea || !shot) return;
  panel.querySelector('.drama-rich-prompt-toolbar')?.remove();
  panel.querySelector('.drama-rich-prompt-frame')?.remove();
  delete textarea.dataset.richEditorReady;
  textarea.hidden = false;
  textarea.style.display = '';
  textarea.classList.remove('drama-rich-prompt-source');
  textarea.value = shot.prompt || '';
  textarea.dataset.promptRich = JSON.stringify(shot.prompt_rich || []);
  core.setupDramaRichPromptEditor(project, shot);
}

function bindVideoHistory(project: ApiProject) {
  const history = document.querySelector<HTMLElement>('.drama-video-history');
  const items = history ? [...history.querySelectorAll<HTMLButtonElement>('.drama-history-item')] : [];
  if (history && items.length) {
    const grid = document.createElement('div');
    grid.className = 'drama-history-grid';
    items.forEach(item => {
      const icon = item.querySelector<HTMLElement>(':scope > span');
      const url = item.dataset.dramaHistoryUrl;
      if (icon && !icon.classList.contains('drama-history-thumb')) {
        icon.className = 'drama-history-thumb';
        icon.innerHTML = url ? `<video src="${url}" muted playsinline preload="metadata" aria-hidden="true"></video><i>▶</i>` : '<i>◌</i>';
      }
      const text = item.querySelector<HTMLElement>(':scope > div');
      if (text) { text.className = 'drama-history-meta'; text.insertAdjacentHTML('beforeend', `<em>${url ? '点击预览' : '生成中'}</em>`); }
      grid.append(item);
    });
    history.querySelector('.section-title')?.after(grid);
  }
  document.querySelectorAll<HTMLElement>('.drama-video-panel [data-drama-history-url]').forEach(element => {
    element.addEventListener('click', () => {
      dramaViewState.videoUrl = element.dataset.dramaHistoryUrl || null;
      refreshVideoPanel(project);
    });
  });
}

function refreshVideoPanel(project: ApiProject) {
  const wrapper = document.createElement('div');
  wrapper.innerHTML = core.dramaDetailMarkup(project);
  const next = wrapper.querySelector<HTMLElement>('.drama-video-panel');
  const current = document.querySelector<HTMLElement>('.drama-video-panel');
  if (next && current) {
    current.replaceWith(next);
    bindVideoHistory(project);
    const shot = core.dramaSelectedShot(project);
    if (shot) core.enhanceDramaShotEditor(project, shot);
  }
}

async function refreshAssets(project: ApiProject) {
  if (!runtime) return;
  const response = await fetch(`${runtime.apiBaseUrl}/projects/${encodeURIComponent(project.id)}/assets`);
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  project.assets = await response.json() as ApiProject['assets'];
  setActiveDramaProject(project);
  window.dispatchEvent(new CustomEvent('drama-assets-refreshed', { detail: project.id }));
  syncDramaCoverUi(project);
  core.syncDramaShotReferencePanel(project);
  const backdrop = document.querySelector<HTMLElement>('.drama-sheet-backdrop');
  if (backdrop && dramaViewState.assetPanel) {
    replaceDramaAssetDrawer(backdrop, core.dramaAssetDrawer(project), () => core.bindDramaAssetDrawer(project));
  }
}

async function refreshShots(project: ApiProject) {
  if (!runtime) return;
  const response = await fetch(`${runtime.apiBaseUrl}/projects/${encodeURIComponent(project.id)}/shots`);
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const payload = await response.json() as { shots?: ApiProject['shots']; episodes?: ApiProject['episodes'] };
  project.shots = payload.shots || [];
  project.episodes = payload.episodes || [];
  setActiveDramaProject(project);
  refreshShotList(project);
}

export async function applyDramaTaskUpdate(tasks: GenerationTask[], completed: GenerationTask[]) {
  const project = activeDramaProject;
  if (!runtime || !project || (!tasks.length && !completed.length)) return;
  mergeTasks(project, [...tasks, ...completed]);
  if (completed.some(task => ['script_decomposition', 'script_expansion'].includes(task.type))) {
    await runtime.loadFullDetail(project.id);
    return;
  }
  const assetChanged = completed.some(task => ['asset_image', 'asset_variant_image', 'placeholder_image', 'cover_image'].includes(task.type));
  const shotChanged = completed.some(task => ['shot_prompt', 'shot_quality', 'shot_video'].includes(task.type));
  if (assetChanged) await refreshAssets(project);
  if (shotChanged) await refreshShots(project);
  const shot = core.dramaSelectedShot(project);
  if (!shot) {
    refreshDramaVideoBatchGeneration(project);
    return;
  }
  if (completed.some(task => task.type === 'shot_video' && task.resource_id === shot.id)) refreshVideoPanel(project);
  core.enhanceDramaShotEditor(project, shot);
  refreshDramaVideoBatchGeneration(project);
  if (completed.some(task => task.type === 'shot_prompt' && task.resource_id === shot.id)) resetRichPromptEditor(project);
}
