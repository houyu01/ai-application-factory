/** Core drama editor UI: renders assets, shots, rich prompts, and durable-task states. */
import type { ApiProject, DramaAsset, DramaAssetImageHistory, DramaAssetKind, DramaAssetMetadata, DramaAssetVariant, DramaPlacement, DramaPromptAssetType, DramaPromptNode, DramaShot, GenerationTask, Project, VoicePreset } from './models.js';
import { dramaAssetImageIsGenerating, dramaImageLoadingMarkup } from './drama_asset_image_state_ui.js';
import { dramaReferenceAsset, dramaReferenceKey, dramaReferenceOptions } from './drama_reference_asset.js';
import './drama_image_viewer.js'; import './drama_asset_editor_modal.css'; import { suppressExistingModelTaskFailureNotifications } from './model_task_failure_toast.js';
import { scheduleDramaTaskRefresh as pollDramaTasks } from './drama_task_polling.js';
import { syncDramaVideoGenerationInfo } from './drama_video_failure_ui.js';
import { refreshDramaVideoCancellation } from './drama_video_cancellation_ui.js';
import { syncDramaVideoPreviewNavigation, syncDramaVideoPreviewStatus } from './drama_video_preview_ui.js';
import { syncDramaBatchVideoCancellation } from './drama_video_batch_cancellation_ui.js'; import { refreshDramaVideoBatchGeneration } from './drama_video_batch_generation_ui.js';
import { syncDramaAssetBatchCancellation } from './drama_asset_batch_cancellation_ui.js';
import { activeDramaProject, dramaViewState, setActiveDramaProject } from './drama_state.js';
import { setupDramaShotDurationControl } from './drama_shot_duration_ui.js';
import { renderDramaQualityPanel } from './drama_quality_ui.js';
import { icon } from './ui_icons.js';
import { resolveDesktopMediaUrl } from './desktop_api.js';
import { bindDramaEpisodeManager } from './drama_episode_ui.js';
import { renderDramaImageHistoryModal } from './drama_image_history_ui.js';
import { renderDramaShotReferencePanel, syncDramaShotReferenceCards } from './drama_reference_panel.js';
import { syncDramaVideoHistoryActions } from './drama_video_history_actions_ui.js';
import { dramaShotVideoStatus, dramaVideoHistoryRecords, latestDramaVideoUrl } from './drama_video_history.js';
import { confirmAction } from './confirmation_modal.js';
import { captureDramaVideoFrame } from './drama_video_frame_capture.js';
import { batchPromptLoadingState, shouldUpdateTaskControl } from './drama_batch_prompt_loading.js';
import { replaceDramaAssetDrawer } from './drama_asset_drawer_refresh.js';
import { syncDramaTaskRetryControls } from './drama_task_retry_ui.js';
import { placeTrailingDramaReferences } from './drama_prompt_reference_placement.js';
import { flushDramaEditorAutosave, registerDramaEditorAutosave } from './drama_editor_autosave.js';
type DramaRuntime = {
  apiBaseUrl: string;
  active: () => string;
  projects: Project[];
  projectFromApi: (project: ApiProject) => Project;
  render: () => void;
  escapeHtml: (value: unknown) => string;
  toast: (message: string) => void;
  loadDramaDetail: (id: string, retry?: number) => Promise<void>;
  loadDramaProjects: () => Promise<void>;
  loadVoicePresets: () => Promise<unknown>;
  voiceOptions: (id?: string | null) => string;
  voicePreset: (id?: string | null) => VoicePreset | null | undefined;
  bindDramaWorkspace: (project: ApiProject) => void;
};
let runtime: DramaRuntime;
const rt = () => runtime;
export function configureDramaRuntime(value: DramaRuntime) { runtime = value; }
export function dramaKindLabel(kind: string) { return ({ character: '角色', scene: '场景', prop: '道具', placeholder: '占位图' } as Record<string, string>)[kind] || kind; }
export function dramaStatusClass(status: string) { return status === '生成中' ? 'running' : status === '生成失败' ? 'failed' : status === '生成成功' ? 'success' : ''; }
export function dramaStatusText(status: string) { return status || '未生成'; }
export function dramaTime(value?: string) { return value ? value.slice(0, 16).replace('T', ' ') : '刚刚'; }
export function dramaAssets(project: ApiProject) { return (project.assets || []).filter(asset => ['character', 'scene', 'prop', 'placeholder'].includes(asset.type) && (asset.type !== 'placeholder' || asset.metadata?.render_mode === 'generated_composite')); }
export function dramaShots(project: ApiProject) { return project.shots || []; }
export function dramaSelectedShot(project: ApiProject) { const shots = dramaShots(project); return shots.find(shot => shot.id === dramaViewState.shotId) || shots[0]; }
export function resolveMediaUrl(value?: string | null) { if (!value) return ''; if (/^(https?:|data:|blob:|media:)/.test(value)) return value; if (value.startsWith('/api/')) return resolveDesktopMediaUrl(`${rt().apiBaseUrl}${value.slice(4)}`); return resolveDesktopMediaUrl(`${rt().apiBaseUrl}${value.startsWith('/') ? value : `/${value}`}`); }
function resolveDramaFrameUrl(value?: string | null) { const normalized = String(value || '').trim().replace(/^['"]|['"]$/g, ''); return normalized ? resolveMediaUrl(normalized) : ''; }
export function dramaReadyAssets(project: ApiProject, type: 'character' | 'scene' | 'prop' | 'placeholder') { return dramaAssets(project).filter(asset => asset.type === type && asset.status === '生成成功' && Boolean(asset.image_url)); }
export function latestDramaTask(project: ApiProject, type: string, resourceId?: string) { return [...(project.tasks || [])].reverse().find(task => task.type === type && (resourceId === undefined || task.resource_id === resourceId)); } export function activeDramaTask(project: ApiProject, type: string, resourceId?: string) { return [...(project.tasks || [])].reverse().find(task => task.type === type && task.status === '生成中' && (resourceId === undefined || task.resource_id === resourceId)); }
export function dramaShotVideoTask(project: ApiProject, shotId: string) { return activeDramaTask(project, 'shot_video', shotId) || latestDramaTask(project, 'shot_video', shotId); }
export function dramaTaskRunning(project: ApiProject, type: string, resourceId?: string) { return Boolean(activeDramaTask(project, type, resourceId)); } export function dramaTaskQueued(project: ApiProject, type: string, resourceId?: string) { return activeDramaTask(project, type, resourceId)?.stage === '等待队列'; }
export function dramaPlaceholderTaskRunning(project: ApiProject, shotId: string) { return (project.tasks || []).some(task => task.type === 'placeholder_image' && task.status === '生成中' && task.input_snapshot?.shot_id === shotId); }
export function setGenerationButtonLoading(button: HTMLButtonElement, loading: boolean, idleText: string) { button.dataset.taskIdleText = idleText; button.disabled = loading || button.dataset.generationEligible === 'false'; button.classList.toggle('is-loading', loading); button.setAttribute('aria-busy', String(loading)); button.innerHTML = loading ? '<span class="generation-spinner" aria-hidden="true"></span><span>生成中...</span>' : idleText; }
export function taskProgressLabel(task?: GenerationTask) {
  if (!task || task.status !== '生成中') return '';
  const progress = Math.max(0, Math.min(100, Number(task.progress || 0)));
  const stage = task.stage ? ` · ${task.stage.replace(/^provider_/, '')}` : '';
  return `${progress}%${stage}`;
}
export function setTaskButtonLoading(button: HTMLButtonElement, task: GenerationTask | undefined, idleText: string, fallbackLoading = false) {
  const loading = Boolean(task && task.status === '生成中') || fallbackLoading;
  setGenerationButtonLoading(button, loading, idleText);
  if (loading && task) {
    button.innerHTML = `<span class="generation-spinner" aria-hidden="true"></span><span>生成中... ${taskProgressLabel(task)}</span>`;
    button.title = task.stage ? `任务进行中：${task.stage}` : '任务进行中，刷新页面后会继续显示进度';
  } else {
    button.removeAttribute('title');
  }
}
export function dramaShotReferences(project: ApiProject, shot: DramaShot) {
  const nodes = dramaPromptNodes(project, shot);
  return nodes.filter((node): node is Extract<DramaPromptNode, { type: 'reference' }> => node.type === 'reference');
}
export function enhanceDramaShotEditor(project: ApiProject, shot: DramaShot) {
  const editor = document.querySelector<HTMLElement>('.drama-shot-editor');
  if (!editor) return;
  const videoTask = dramaShotVideoTask(project, shot.id);
  const promptTask = latestDramaTask(project, 'shot_prompt', shot.id);
  const qualityTask = latestDramaTask(project, 'shot_quality', shot.id);
  const referenceImageTask = latestDramaTask(project, 'shot_reference_image_batch', shot.id);
  const referencePanel = editor.querySelector<HTMLElement>('.drama-reference-panel');
  renderDramaShotReferencePanel({ project, shot, referenceImageTask, escapeHtml: rt().escapeHtml, setTaskButtonLoading });
  const quickAssets = referencePanel?.querySelector<HTMLElement>('.drama-quick-assets');
  if (quickAssets) {
    quickAssets.querySelectorAll('[data-drama-open-assets]').forEach(button => button.remove());
    if (!quickAssets.querySelector('[data-drama-add-reference]')) {
      const addReference = document.createElement('button');
      addReference.type = 'button';
      addReference.className = 'ghost compact';
      addReference.dataset.dramaAddReference = 'true';
      addReference.textContent = '＋ 添加参考图';
      quickAssets.append(addReference);
    }
  }
  const promptPanel = editor.querySelector<HTMLElement>('.drama-prompt-panel');
  if (promptPanel) renderDramaQualityPanel({ promptPanel, shot, qualityTask, escapeHtml: rt().escapeHtml, taskProgressLabel });
  const videoPanel = editor.parentElement?.querySelector<HTMLElement>('.drama-video-panel');
  syncDramaVideoPreviewStatus(videoPanel, shot, videoTask);
  syncDramaVideoPreviewNavigation(videoPanel, project, shot, videoTask, shotId => { void flushDramaEditorAutosave().then(() => { dramaViewState.shotId = shotId; dramaViewState.videoUrl = null; return rt().loadDramaDetail(project.id); }).catch(() => undefined); });
  syncDramaVideoGenerationInfo(videoPanel, shot, videoTask, rt().escapeHtml);
  const videoButton = document.querySelector<HTMLButtonElement>('#drama-generate-shot-video');
  if (videoButton) setTaskButtonLoading(videoButton, videoTask, '▣ 生成视频');
  const promptButton = document.querySelector<HTMLButtonElement>('#drama-generate-shot-prompt');
  if (promptButton) setTaskButtonLoading(promptButton, promptTask, '✣ 生成提示词');
  const qualityButton = editor.querySelector<HTMLButtonElement>('[data-drama-quality-check]');
  if (qualityButton) setTaskButtonLoading(qualityButton, qualityTask, '运行检查');
  refreshDramaVideoCancellation(project, shot);
}
export function applyDramaGenerationLoading(project: ApiProject, updatedTypes?: ReadonlySet<string>) {
  document.querySelectorAll<HTMLButtonElement>('[data-drama-generate-asset]').forEach(button => { const asset = dramaAssets(project).find(item => item.id === button.dataset.dramaGenerateAsset); if (asset) setGenerationButtonLoading(button, dramaTaskRunning(project, 'asset_image', asset.id) || asset.status === '生成中', `${icon('sparkle')}<span>生成图片</span>`); });
  document.querySelectorAll<HTMLButtonElement>('[data-drama-generate-variant]').forEach(button => { const variant = dramaAssets(project).flatMap(asset => asset.variants || []).find(item => item.id === button.dataset.dramaVariantId); setGenerationButtonLoading(button, dramaTaskRunning(project, 'asset_variant_image', button.dataset.dramaVariantId) || variant?.status === '生成中', `${icon('sparkle')}<span>生成图片</span>`); });
  const kind = dramaViewState.assetPanel;
  const kindAssets = kind ? dramaAssets(project).filter(asset => asset.type === kind) : [];
  const allAssetLoading = Boolean(kind && dramaTaskRunning(project, 'asset_image_batch', kind)) || kindAssets.some(asset => dramaTaskRunning(project, 'asset_image', asset.id) || asset.status === '生成中' || (asset.variants || []).some(variant => dramaTaskRunning(project, 'asset_variant_image', variant.id) || variant.status === '生成中'));
  const allAssetsButton = document.querySelector<HTMLButtonElement>('[data-drama-generate-all-assets]'); if (allAssetsButton) setGenerationButtonLoading(allAssetsButton, allAssetLoading, `<span class="drama-button-icon">${icon('image')}</span><span>生成全部图片</span>`);
  syncDramaAssetBatchCancellation({ apiBaseUrl: rt().apiBaseUrl, project, assetType: kind, toast: rt().toast, reloadProject: rt().loadDramaDetail });
  const shot = dramaSelectedShot(project);
  if (shot) {
    const promptButton = document.querySelector<HTMLButtonElement>('#drama-generate-shot-prompt'); if (promptButton) setGenerationButtonLoading(promptButton, dramaTaskRunning(project, 'shot_prompt', shot.id), '✣ 生成提示词');
    const videoButton = document.querySelector<HTMLButtonElement>('#drama-generate-shot-video'); if (videoButton) setGenerationButtonLoading(videoButton, dramaTaskRunning(project, 'shot_video', shot.id), '▣ 生成视频');
    const placeholderButton = document.querySelector<HTMLButtonElement>('[data-placeholder-generate]'); if (placeholderButton) setGenerationButtonLoading(placeholderButton, dramaPlaceholderTaskRunning(project, shot.id), `${icon('image')}<span>生成占位图</span>`);
  }
  const allPromptButton = document.querySelector<HTMLButtonElement>('#drama-generate-all-prompts');
  if (allPromptButton && shouldUpdateTaskControl(updatedTypes, 'shot_prompt')) {
    const state = batchPromptLoadingState(project);
    setGenerationButtonLoading(allPromptButton, state.loading, '✣ 批量生成提示词');
    allPromptButton.title = state.queuedCount ? `${state.queuedCount} 条提示词正在等待语言模型队列` : '';
  }
  const allVideoButton = document.querySelector<HTMLButtonElement>('#drama-generate-all-videos'); if (allVideoButton) setGenerationButtonLoading(allVideoButton, dramaShots(project).some(item => dramaTaskRunning(project, 'shot_video', item.id)), '▣ 生成所有视频');
  refreshDramaVideoBatchGeneration(project);
  syncDramaBatchVideoCancellation({ apiBaseUrl: rt().apiBaseUrl, project, toast: rt().toast, reloadProject: rt().loadDramaDetail });
  if (shot) enhanceDramaShotEditor(project, shot); syncDramaTaskRetryControls(project, shot?.id); syncDramaVideoHistoryActions({ apiBaseUrl: rt().apiBaseUrl, project, resolveMediaUrl, loadDramaDetail: rt().loadDramaDetail, toast: rt().toast });
}
export function scheduleDramaTaskRefresh(project: ApiProject) { pollDramaTasks(project); }
document.addEventListener('click', event => { const target = event.target instanceof Element ? event.target.closest<HTMLElement>('[data-drama-close-sheet], [data-drama-sheet-backdrop]') : null; if (!target || (target.hasAttribute('data-drama-sheet-backdrop') && event.target !== target)) return; event.preventDefault(); event.stopImmediatePropagation(); dramaViewState.assetPanel = null; const main = document.querySelector('main'); if (main && activeDramaProject) { main.innerHTML = dramaDetailMarkup(activeDramaProject); rt().bindDramaWorkspace(activeDramaProject); } }, true);
export function normalizeDramaPlacement(placement: Partial<DramaPlacement>, index = 0): DramaPlacement { const rawWidth = Number(placement.width); const rawHeight = Number(placement.height); const rawX = Number(placement.x); const rawY = Number(placement.y); const width = Math.min(1, Math.max(0.04, Number.isFinite(rawWidth) && rawWidth > 0 ? rawWidth : 0.2)); const height = Math.min(1, Math.max(0.04, Number.isFinite(rawHeight) && rawHeight > 0 ? rawHeight : 0.35)); const x = Math.min(1 - width, Math.max(0, Number.isFinite(rawX) ? rawX : Math.min(0.72, 0.28 + index * 0.16))); const y = Math.min(1 - height, Math.max(0, Number.isFinite(rawY) ? rawY : Math.min(0.62, 0.26 + index * 0.08))); return { id: placement.id || `placement-${Date.now()}-${index}`, asset_id: placement.asset_id || '', x, y, width, height, pose: placement.pose || '', note: placement.note || placement.pose || '' }; }
export function syncDramaShotReferencePanel(project: ApiProject) { syncDramaShotReferenceCards(project, rt().escapeHtml); }
export function dramaAssetImageMarkup(asset: DramaAsset | DramaAssetVariant, tasks: GenerationTask[] = []) {
  if (dramaAssetImageIsGenerating(asset, tasks)) return dramaImageLoadingMarkup(asset.name, rt().escapeHtml);
  if (asset.image_url) { const url = resolveMediaUrl(asset.image_url); return `<button type="button" class="drama-image-preview-trigger" data-drama-image-preview="${rt().escapeHtml(url)}" data-drama-image-label="${rt().escapeHtml(asset.name)}" aria-label="查看${rt().escapeHtml(asset.name)}图片"><img src="${rt().escapeHtml(url)}" alt="${rt().escapeHtml(asset.name)}" /></button>`; }
  const type = 'type' in asset ? asset.type : 'character';
  return `<div class="drama-asset-placeholder">${type === 'character' ? icon('character') : type === 'scene' ? '✦' : type === 'placeholder' ? '▱' : '◆'}</div>`;
}
export function dramaImageHistoryButton(asset: DramaAsset | DramaAssetVariant, parentAssetId = asset.id) {
  const count = asset.image_history?.length || 0;
  return `<button class="ghost compact" data-drama-image-history="${rt().escapeHtml(asset.id)}" data-drama-parent-asset="${rt().escapeHtml(parentAssetId)}" ${count ? '' : 'disabled'}>${icon('history')}<span>图片历史${count ? `（${count}）` : ''}</span></button>`;
}
export function dramaAssetVariantCard(asset: DramaAsset, variant: DramaAssetVariant, tasks: GenerationTask[] = []) {
  return `<article class="drama-asset-variant-card" data-drama-variant-card="${rt().escapeHtml(variant.id)}"><div class="drama-asset-variant-image">${dramaAssetImageMarkup(variant, tasks)}</div><div class="drama-asset-variant-body"><div class="drama-asset-heading"><div><h4>${rt().escapeHtml(variant.name)}</h4><span>${rt().escapeHtml(variant.id.slice(-8))}</span></div><span class="status ${dramaStatusClass(variant.status)}">${rt().escapeHtml(dramaStatusText(variant.status))}</span></div><p>${rt().escapeHtml(variant.prompt || '等待生成形态提示词')}</p><div class="drama-asset-actions"><button class="small-btn" data-drama-generate-variant="${rt().escapeHtml(asset.id)}" data-drama-variant-id="${rt().escapeHtml(variant.id)}">${variant.status === '生成中' ? '生成中…' : `${icon('sparkle')}<span>生成图片</span>`}</button><button class="ghost compact" data-drama-edit-variant="${rt().escapeHtml(asset.id)}" data-drama-variant-id="${rt().escapeHtml(variant.id)}">${icon('edit')}<span>编辑</span></button>${dramaImageHistoryButton(variant, asset.id)}<button class="danger-button compact" data-drama-delete-variant="${rt().escapeHtml(asset.id)}" data-drama-variant-id="${rt().escapeHtml(variant.id)}">${icon('trash')}<span>删除</span></button></div></div></article>`;
}
/** Voice selection remains available in the asset editor, not on compact management cards. */
export function dramaAssetVoiceField(_asset: DramaAsset) { return ''; }
export function dramaAssetCard(project: ApiProject, asset: DramaAsset) {
  const variants = asset.variants || [];
  const imageTask = latestDramaTask(project, 'asset_image', asset.id);
  const failureReason = asset.status === '生成失败' && imageTask?.status === '生成失败' ? imageTask.error_message?.trim() : '';
  const statusTitle = failureReason ? `生成失败：${failureReason}` : undefined;
  const variantAction = `<button class="ghost compact" data-drama-add-variant="${rt().escapeHtml(asset.id)}">${icon('plus')}<span>添加形态</span></button>${asset.type === 'character' ? `<button class="ghost compact" data-drama-change-outfit="${rt().escapeHtml(asset.id)}">${icon('shirt')}<span>换装</span></button>` : ''}`;
  const variantManagement = `<details class="drama-asset-variants" ${variants.length ? '' : 'hidden'}><summary>展开其他形态 <span>${variants.length} 个其他形态</span></summary><div class="drama-asset-variant-list">${variants.map(variant => dramaAssetVariantCard(asset, variant, project.tasks)).join('')}</div></details>`;
  return `<article class="drama-asset-card" data-drama-asset-card="${rt().escapeHtml(asset.id)}" data-asset-type="${rt().escapeHtml(asset.type)}" data-asset-name="${rt().escapeHtml(asset.name.toLowerCase())}"><div class="drama-asset-main"><div class="drama-asset-image">${dramaAssetImageMarkup(asset, project.tasks)}</div><div class="drama-asset-body"><div class="drama-asset-heading"><div><h3>${rt().escapeHtml(asset.name)} <small>${rt().escapeHtml(asset.id.slice(-8))}</small></h3><span>${dramaKindLabel(asset.type)} · 基础形态</span></div><div class="drama-asset-card-tools">${asset.image_url ? `<a class="drama-icon-button" href="${rt().escapeHtml(resolveMediaUrl(asset.image_url))}" download target="_blank" rel="noopener" title="下载图片">${icon('download')}</a>` : ''}<button class="drama-icon-button" data-drama-edit-asset="${rt().escapeHtml(asset.id)}" title="编辑">${icon('edit')}</button><button class="drama-icon-button danger" data-drama-delete-asset="${rt().escapeHtml(asset.id)}" title="删除">${icon('trash')}</button></div></div><div class="drama-asset-badges"><span class="status ${dramaStatusClass(asset.status)}"${statusTitle ? ` title="${rt().escapeHtml(statusTitle)}"` : ''}>${rt().escapeHtml(dramaStatusText(asset.status))}</span><span class="drama-asset-form-badge">基础形态</span></div>${failureReason ? `<div class="drama-asset-error"><b>生成失败原因：</b>${rt().escapeHtml(failureReason)}</div>` : ''}${dramaAssetVoiceField(asset)}<p class="drama-asset-alias"><b>别名：</b>${rt().escapeHtml(asset.name)} / ${rt().escapeHtml(asset.name)}</p><p class="drama-asset-prompt"><b>图片提示词：</b>${rt().escapeHtml(asset.prompt || '等待生成素材提示词')}</p>${asset.type === 'character' ? '' : variantManagement}</div></div><div class="drama-asset-actions"><button class="small-btn" data-drama-generate-asset="${rt().escapeHtml(asset.id)}">${asset.status === '生成中' ? '生成中…' : `${icon('sparkle')}<span>生成图片</span>`}</button><button class="ghost compact" data-drama-upload-asset="${rt().escapeHtml(asset.id)}">${icon('upload')}<span>上传${dramaKindLabel(asset.type)}</span></button>${variantAction}${dramaImageHistoryButton(asset)}</div>${asset.type === 'character' ? variantManagement : ''}</article>`;
}
export function dramaAssetDrawer(project: ApiProject) {
  const kind = dramaViewState.assetPanel || 'character';
  const assets = dramaAssets(project).filter(asset => asset.type === kind);
  const completedCount = assets.filter(asset => asset.status === '生成成功').length; const isOpening = !document.querySelector('.drama-asset-sheet');
  return `<div class="drama-sheet-backdrop" data-drama-sheet-backdrop><aside class="drama-asset-sheet${isOpening ? ' is-opening' : ''}"><div class="drama-sheet-head"><div><div class="eyebrow">素材库 / ${dramaKindLabel(kind)}</div><h2>${dramaKindLabel(kind)}素材 <span class="sheet-badge">${completedCount === assets.length && assets.length ? '已完成' : '生成中'}</span></h2><p>共 ${assets.length} 个素材${assets.length ? `，${completedCount} 个已完成` : ''}</p></div><button class="close sheet-close" data-drama-close-sheet>×</button></div><div class="drama-sheet-tabs">${(['character', 'scene', 'prop'] as DramaAssetKind[]).map(item => `<button class="${item === kind ? 'active' : ''}" data-drama-asset-tab="${item}">${dramaKindLabel(item)} <small>${dramaAssets(project).filter(asset => asset.type === item).length}</small></button>`).join('')}</div><div class="drama-sheet-toolbar drama-sheet-toolbar-primary"><button class="primary drama-sheet-button" data-drama-generate-all-assets><span class="drama-button-icon">${icon('image')}</span><span>生成全部图片</span></button><button class="ghost danger-button drama-sheet-button" data-drama-cancel-asset-images>取消全部生成</button><button class="ghost drama-sheet-button" data-drama-open-asset-public><span class="drama-button-icon">${icon('square')}</span><span>公共提示词</span></button></div><div class="drama-sheet-toolbar drama-sheet-toolbar-secondary"><button class="ghost drama-sheet-button" data-drama-add-asset="${kind}"><span class="drama-button-icon">${icon('plus')}</span><span>添加${dramaKindLabel(kind)}</span></button><button class="ghost drama-sheet-button" data-drama-refresh><span class="drama-button-icon">${icon('refresh')}</span><span>刷新</span></button><span class="drama-sheet-toolbar-spacer"></span><button class="ghost compact drama-sheet-button" data-drama-collapse-assets><span class="drama-button-icon">${icon('collapse')}</span><span>收起</span></button><button class="ghost compact drama-sheet-button drama-sheet-icon-button" data-drama-toggle-search aria-label="搜索"><span class="drama-button-icon">${icon('search')}</span></button><button class="ghost compact drama-sheet-button drama-sheet-icon-button" data-drama-toggle-filter aria-label="筛选"><span class="drama-button-icon">${icon('sliders')}</span></button></div><div class="drama-asset-search" hidden><input data-drama-asset-search placeholder="搜索${dramaKindLabel(kind)}名称" /></div><div class="drama-sheet-list">${assets.length ? assets.map(asset => dramaAssetCard(project, asset)).join('') : `<div class="drama-sheet-empty"><div class="empty-icon">${kind === 'character' ? '♙' : kind === 'scene' ? '✦' : '◆'}</div><p>还没有${dramaKindLabel(kind)}素材</p><button class="primary drama-sheet-button" data-drama-add-asset="${kind}"><span class="drama-button-icon">${icon('plus')}</span><span>添加${dramaKindLabel(kind)}</span></button></div>`}</div></aside></div>`;
}
export function openDramaPlaceholderModal(project: ApiProject) {
  const shot = dramaSelectedShot(project);
  if (!shot) {
    rt().toast('当前没有可编辑的分镜');
    return;
  }
  const readyScenes = () => dramaReadyAssets(project, 'scene');
  const readyRoles = () => dramaReadyAssets(project, 'character');
  let selectedSceneId = shot.placeholder_scene_asset_id || readyScenes()[0]?.id || '';
  if (!readyScenes().some(asset => asset.id === selectedSceneId)) selectedSceneId = readyScenes()[0]?.id || '';
  let placements = (shot.placeholder_placements || []).map((item, index) => normalizeDramaPlacement(item, index)).filter(item => item.asset_id);
  let dragState: { id: string; startX: number; startY: number; baseX: number; baseY: number; width: number; height: number; canvasWidth: number; canvasHeight: number } | null = null;
  let pendingPlaceholderId = '';
  let pendingPlaceholder: DramaAsset | null = null;
  let onAssetsRefreshed: (event: Event) => void = () => {};
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop drama-placeholder-backdrop';
  const close = () => { window.removeEventListener('drama-assets-refreshed', onAssetsRefreshed); modal.remove(); };
  const currentScene = () => readyScenes().find(asset => asset.id === selectedSceneId) || null;
  const history = () => [...(pendingPlaceholder ? [pendingPlaceholder] : []), ...dramaAssets(project).filter(asset => asset.type === 'placeholder' && asset.metadata?.shot_id === shot.id && asset.id !== pendingPlaceholder?.id)].sort((a, b) => a.id === pendingPlaceholderId ? -1 : b.id === pendingPlaceholderId ? 1 : String(b.updated_at || '').localeCompare(String(a.updated_at || '')));
  const render = () => {
    const scene = currentScene();
    const roleAssets = readyRoles();
    const placeholderHistory = history();
    modal.innerHTML = `<div class="modal drama-placeholder-modal"><div class="modal-head"><button class="close" data-placeholder-close>×</button><h2>占位图</h2><p>为当前分镜设置角色在场景中的相对位置，生成后可作为分镜视频参考图。</p></div><div class="drama-placeholder-body"><div class="drama-placeholder-main"><div class="drama-placeholder-canvas-card"><div class="drama-placeholder-canvas-head"><select data-placeholder-scene ${scene ? '' : 'disabled'}>${readyScenes().length ? readyScenes().map(asset => `<option value="${rt().escapeHtml(asset.id)}" ${asset.id === selectedSceneId ? 'selected' : ''}>${rt().escapeHtml(asset.name)}</option>`).join('') : '<option>请选择已生成场景</option>'}</select><span>已放置 ${placements.length} 个角色</span></div><div class="drama-placeholder-canvas ${project.ratio === '9:16' ? 'portrait' : 'landscape'}" data-placeholder-canvas>${scene?.image_url ? `<img src="${rt().escapeHtml(resolveMediaUrl(scene.image_url))}" data-drama-image-preview="${rt().escapeHtml(resolveMediaUrl(scene.image_url))}" data-drama-image-label="${rt().escapeHtml(scene.name)}" alt="${rt().escapeHtml(scene.name)}" />` : '<div class="drama-placeholder-canvas-empty">请先选择一张已生成的场景图片</div>'}${placements.map((placement, index) => { const role = dramaAssets(project).find(asset => asset.id === placement.asset_id); return `<button type="button" class="drama-placeholder-box" data-placeholder-drag="${rt().escapeHtml(placement.id)}" style="left:${placement.x * 100}%;top:${placement.y * 100}%;width:${placement.width * 100}%;height:${placement.height * 100}%"><b>${String.fromCharCode(65 + index % 26)}</b><span>${rt().escapeHtml(role?.name || '角色')}</span></button>`; }).join('')}</div><p class="drama-placeholder-hint">拖动画布中的橙色框调整人物位置和相对大小。橙色框与字母仅用于视频模型参考，不会要求模型绘制到最终视频中。</p></div>${placeholderHistory.length ? `<div class="drama-placeholder-history"><div class="section-title"><div><h3>占位图历史</h3><p>每次生成都会保留一个版本。</p></div><span>${placeholderHistory.length} 个版本</span></div><div class="drama-placeholder-history-grid">${placeholderHistory.map(asset => `<div class="drama-placeholder-history-card">${asset.image_url ? `<button type="button" class="drama-placeholder-image-preview" data-drama-image-preview="${rt().escapeHtml(resolveMediaUrl(asset.image_url))}" data-drama-image-label="${rt().escapeHtml(asset.name)}"><img src="${rt().escapeHtml(resolveMediaUrl(asset.image_url))}" alt="${rt().escapeHtml(asset.name)}" /></button>` : '<div class="drama-placeholder-history-empty">生成中…</div>'}<small>${rt().escapeHtml(asset.name)} · ${rt().escapeHtml(dramaStatusText(asset.status))}</small></div>`).join('')}</div></div>` : ''}</div><div class="drama-placeholder-side"><section class="drama-placeholder-section"><div class="section-title"><div><h3>场景</h3><p>选择已生成的场景作为背景。</p></div><span>${readyScenes().length} 个可用</span></div>${readyScenes().length ? `<div class="drama-placeholder-scene-list">${readyScenes().map(asset => `<button type="button" class="drama-placeholder-scene-option ${asset.id === selectedSceneId ? 'selected' : ''}" data-placeholder-scene-option="${rt().escapeHtml(asset.id)}"><span>${asset.image_url ? `<img src="${rt().escapeHtml(resolveMediaUrl(asset.image_url))}" data-drama-image-preview="${rt().escapeHtml(resolveMediaUrl(asset.image_url))}" data-drama-image-label="${rt().escapeHtml(asset.name)}" alt="" />` : '✦'}</span><b>${rt().escapeHtml(asset.name)}</b></button>`).join('')}</div>` : '<div class="drama-placeholder-empty">当前还没有已生成的场景图。</div>'}</section><section class="drama-placeholder-section"><div class="section-title"><div><h3>角色</h3><p>点击角色添加到场景中。</p></div><span>${roleAssets.length} 个可用</span></div>${roleAssets.length ? `<div class="drama-placeholder-role-list">${roleAssets.map(asset => { const count = placements.filter(item => item.asset_id === asset.id).length; return `<button type="button" class="drama-placeholder-role-option" data-placeholder-add-role="${rt().escapeHtml(asset.id)}"><span>${asset.image_url ? `<img src="${rt().escapeHtml(resolveMediaUrl(asset.image_url))}" data-drama-image-preview="${rt().escapeHtml(resolveMediaUrl(asset.image_url))}" data-drama-image-label="${rt().escapeHtml(asset.name)}" alt="" />` : '♙'}</span><div><b>${rt().escapeHtml(asset.name)}</b><small>${count ? `已放置 ${count} 个` : '添加到占位图'}</small></div><strong>＋</strong></button>`; }).join('')}</div>` : '<div class="drama-placeholder-empty">当前还没有已生成的角色图。</div>'}</section><section class="drama-placeholder-section"><div class="section-title"><div><h3>占位框</h3><p>可以删除角色或补充动作备注。</p></div><button type="button" class="ghost compact" data-placeholder-clear ${placements.length ? '' : 'disabled'}>清空</button></div>${placements.length ? `<div class="drama-placeholder-placement-list">${placements.map((placement, index) => { const role = dramaAssets(project).find(asset => asset.id === placement.asset_id); return `<div class="drama-placeholder-placement-item"><div><b>${String.fromCharCode(65 + index % 26)} · ${rt().escapeHtml(role?.name || '角色')}</b><button type="button" class="drama-placeholder-remove" data-placeholder-remove="${rt().escapeHtml(placement.id)}" aria-label="删除该角色" title="删除">${icon('trash')}</button></div><input data-placeholder-note="${rt().escapeHtml(placement.id)}" value="${rt().escapeHtml(placement.note || placement.pose || '')}" placeholder="动作或位置备注" /></div>`; }).join('')}</div>` : '<div class="drama-placeholder-empty">还没有占位框，请从上方角色列表添加。</div>'}</section></div></div></div><div class="modal-actions"><button type="button" class="ghost" data-placeholder-cancel>取消</button><button type="button" class="ghost" data-placeholder-save>保存草稿</button><button type="button" class="primary" data-placeholder-generate ${scene && placements.length ? '' : 'disabled'}>${icon('image')}<span>生成占位图</span></button></div></div>`;
    modal.querySelectorAll<HTMLElement>('.drama-placeholder-history-card small').forEach((label, index) => { const item = placeholderHistory[index]; label.textContent = `版本 ${item?.metadata?.version || placeholderHistory.length - index} · ${dramaStatusText(item?.status || '')}`; });
    const draftHint = modal.querySelector<HTMLElement>('.drama-placeholder-hint'); if (draftHint) draftHint.textContent = '橙色框和字母只用于编辑构图草稿；生成时会结合场景、角色和相关道具图片，由图像模型生成无框、无标记的干净参考图。';
    const placeholderDialog = modal.querySelector<HTMLElement>('.drama-placeholder-modal');
    const placeholderActions = modal.querySelector<HTMLElement>(':scope > .modal-actions');
    if (placeholderDialog && placeholderActions) placeholderDialog.appendChild(placeholderActions);
    const placeholderGenerateButton = modal.querySelector<HTMLButtonElement>('[data-placeholder-generate]');
    if (placeholderGenerateButton) { placeholderGenerateButton.dataset.generationEligible = String(Boolean(scene && placements.length)); setGenerationButtonLoading(placeholderGenerateButton, dramaPlaceholderTaskRunning(project, shot.id), `${icon('image')}<span>生成占位图</span>`); }
    modal.querySelector('[data-placeholder-close]')?.addEventListener('click', close);
    modal.querySelector('[data-placeholder-cancel]')?.addEventListener('click', close);
    modal.querySelector<HTMLSelectElement>('[data-placeholder-scene]')?.addEventListener('change', event => { selectedSceneId = (event.target as HTMLSelectElement).value; render(); });
    modal.querySelectorAll<HTMLElement>('[data-placeholder-scene-option]').forEach(button => button.addEventListener('click', () => { selectedSceneId = button.dataset.placeholderSceneOption || ''; render(); }));
    modal.querySelectorAll<HTMLElement>('[data-placeholder-add-role]').forEach(button => button.addEventListener('click', () => { const assetId = button.dataset.placeholderAddRole || ''; if (!assetId) return; const index = placements.length; placements = [...placements, normalizeDramaPlacement({ asset_id: assetId }, index)]; render(); }));
    modal.querySelectorAll<HTMLElement>('[data-placeholder-remove]').forEach(button => button.addEventListener('click', () => { placements = placements.filter(item => item.id !== button.dataset.placeholderRemove); render(); }));
    modal.querySelector('[data-placeholder-clear]')?.addEventListener('click', () => { placements = []; render(); });
    modal.querySelectorAll<HTMLInputElement>('[data-placeholder-note]').forEach(input => input.addEventListener('change', () => { const placement = placements.find(item => item.id === input.dataset.placeholderNote); if (placement) { placement.note = input.value; placement.pose = input.value; } }));
    modal.querySelectorAll<HTMLElement>('[data-placeholder-drag]').forEach(button => {
      button.addEventListener('pointerdown', event => {
        const placement = placements.find(item => item.id === button.dataset.placeholderDrag);
        const canvas = modal.querySelector<HTMLElement>('[data-placeholder-canvas]');
        if (!placement || !canvas) return;
        event.preventDefault();
        const rect = canvas.getBoundingClientRect();
        dragState = { id: placement.id, startX: event.clientX, startY: event.clientY, baseX: placement.x, baseY: placement.y, width: placement.width, height: placement.height, canvasWidth: rect.width, canvasHeight: rect.height };
        button.setPointerCapture?.(event.pointerId);
      });
      button.addEventListener('pointermove', event => {
        if (!dragState || dragState.id !== button.dataset.placeholderDrag) return;
        const placement = placements.find(item => item.id === dragState?.id);
        if (!placement) return;
        placement.x = Math.min(1 - dragState.width, Math.max(0, dragState.baseX + (event.clientX - dragState.startX) / dragState.canvasWidth));
        placement.y = Math.min(1 - dragState.height, Math.max(0, dragState.baseY + (event.clientY - dragState.startY) / dragState.canvasHeight));
        button.style.left = `${placement.x * 100}%`;
        button.style.top = `${placement.y * 100}%`;
      });
      button.addEventListener('pointerup', () => { dragState = null; });
      button.addEventListener('pointercancel', () => { dragState = null; });
    });
    modal.querySelector('[data-placeholder-save]')?.addEventListener('click', async () => { if (!selectedSceneId) { rt().toast('请先选择已生成的场景图'); return; } const response = await fetch(`${rt().apiBaseUrl}/projects/${project.id}/shots/${shot.id}/placeholder-layout`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ shot_id: shot.id, scene_asset_id: selectedSceneId, placements }) }); if (response.ok) { shot.placeholder_scene_asset_id = selectedSceneId; shot.placeholder_placements = placements; rt().toast('占位图布局草稿已保存'); } else rt().toast('占位图布局保存失败'); });
    modal.querySelector('[data-placeholder-generate]')?.addEventListener('click', async () => { const button = modal.querySelector<HTMLButtonElement>('[data-placeholder-generate]'); if (!selectedSceneId || !placements.length || !button) return; setGenerationButtonLoading(button, true, `${icon('image')}<span>生成占位图</span>`); try { const saveResponse = await fetch(`${rt().apiBaseUrl}/projects/${project.id}/shots/${shot.id}/placeholder-layout`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ shot_id: shot.id, scene_asset_id: selectedSceneId, placements }) }); if (!saveResponse.ok) throw new Error('布局保存失败'); const response = await fetch(`${rt().apiBaseUrl}/projects/${project.id}/placeholders/image`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ shot_id: shot.id, scene_asset_id: selectedSceneId, placements }) }); if (!response.ok) { const detail = await response.json().catch(() => ({})); throw new Error(detail.detail || '占位图任务创建失败'); } const task = await response.json() as GenerationTask; pendingPlaceholderId = task.resource_id || task.id; pendingPlaceholder = { id: pendingPlaceholderId, type: 'placeholder', name: '新占位图', prompt: '', metadata: { shot_id: shot.id }, status: '生成中' }; if (activeDramaProject && task.id) { activeDramaProject.tasks = [...(activeDramaProject.tasks || []).filter(item => item.id !== task.id), task]; scheduleDramaTaskRefresh(activeDramaProject); } render(); void syncPlaceholderHistory(); rt().toast('占位图生成任务已创建'); } catch (error) { setGenerationButtonLoading(button, false, `${icon('image')}<span>生成占位图</span>`); rt().toast(error instanceof Error ? error.message : '占位图生成失败'); } });
    modal.addEventListener('click', event => { if (event.target === modal) close(); });
  };
  const syncPlaceholderHistory = async () => { const response = await fetch(`${rt().apiBaseUrl}/projects/${project.id}/assets`); if (!response.ok) return; const assets = await response.json() as DramaAsset[]; project.assets = assets; if (activeDramaProject?.id === project.id) activeDramaProject.assets = assets; pendingPlaceholder = assets.find(asset => asset.id === pendingPlaceholderId) || pendingPlaceholder; render(); };
  onAssetsRefreshed = event => { if ((event as CustomEvent<string>).detail === project.id && modal.isConnected) { pendingPlaceholder = dramaAssets(project).find(asset => asset.id === pendingPlaceholderId) || pendingPlaceholder; render(); } };
  document.body.append(modal);
  window.addEventListener('drama-assets-refreshed', onAssetsRefreshed);
  render();
}
export function dramaDetailMarkup(project: ApiProject) {
  setActiveDramaProject(project);
  registerDramaEditorAutosave(project);
  const shots = dramaShots(project);
  const episodes = project.episodes || [];
  const shot = dramaSelectedShot(project);
  const history = shot ? dramaVideoHistoryRecords(shot) : [];
  const videoTask = shot ? dramaShotVideoTask(project, shot.id) : undefined;
  const shotStatus = shot ? dramaShotVideoStatus(shot, videoTask) : '未生成';
  const selectedVideo = dramaViewState.videoUrl !== null ? dramaViewState.videoUrl : latestDramaVideoUrl(shot);
  const selectedVideoSrc = resolveMediaUrl(selectedVideo);
  const allAssets = dramaAssets(project);
  if (shot) queueMicrotask(() => { const textarea = document.querySelector<HTMLTextAreaElement>('#drama-shot-prompt'); if (activeDramaProject?.id !== project.id || textarea?.dataset.dramaPromptShotId !== shot.id) return; setupDramaShotDurationControl(project, shot, rt()); setupDramaRichPromptEditor(project, shot); });
  return `<div class="drama-detail"><div class="drama-detail-toolbar"><button class="back" id="drama-back">← 返回</button><div class="drama-project-field"><input id="drama-project-name" value="${rt().escapeHtml(project.name)}" maxlength="200" aria-label="短剧标题" autocomplete="off" /></div></div><div class="drama-detail-actions"><button class="ghost" id="drama-generate-all-prompts">✣ 批量生成提示词</button><div class="drama-video-batch-actions" data-drama-video-batch-actions><button class="primary" id="drama-generate-all-videos">▣ 生成所有视频</button><button class="primary drama-video-batch-toggle" type="button" data-drama-video-batch-toggle aria-label="选择视频生成方式" aria-haspopup="true" aria-expanded="false"></button><div class="drama-video-batch-menu" data-drama-video-batch-menu hidden><button type="button" data-drama-generate-videos-serial>串行生成</button><button type="button" data-drama-generate-videos-parallel>并行生成</button></div></div><button class="ghost danger-button" id="drama-cancel-all-videos">取消所有视频任务</button></div><div class="drama-workspace-grid"><section class="panel drama-episode-panel"><div class="panel-title"><div><h2>剧集 / 分镜</h2><p>${episodes.length} 集 · ${shots.length} 条分镜</p></div><button class="ghost compact" id="drama-add-episode">＋ 新增剧集</button></div><div class="drama-episode-list">${episodes.length ? episodes.map(episode => { const episodeShots = shots.filter(item => item.episode_id === episode.id || item.episode_name === episode.title); return `<div class="drama-episode-block"><div class="drama-episode-head"><div><strong>${rt().escapeHtml(episode.title)}</strong><small>${episodeShots.length} 条分镜</small></div><span>⌃</span></div>${episodeShots.map(item => { const status = dramaShotVideoStatus(item, dramaShotVideoTask(project, item.id)); return `<div class="drama-shot-item ${item.id === shot?.id ? 'selected' : ''}"><button class="drama-shot-nav" data-drama-shot="${item.id}"><div><b>#${item.shot_index || 1}</b><span class="status ${dramaStatusClass(status)}">${rt().escapeHtml(dramaStatusText(status))}</span></div><p>${rt().escapeHtml(item.original_text || '还没有填写分镜文本')}</p></button><div class="drama-shot-item-actions"><button type="button" class="drama-shot-action" data-drama-add-shot="${item.id}" title="在当前分镜下添加">＋</button><button type="button" class="drama-shot-action danger" data-drama-delete-shot="${item.id}" title="删除当前分镜">🗑</button></div></div>`; }).join('')}</div>`; }).join('') : `<div class="drama-generating"><div class="empty-icon">◌</div><p>分镜正在后台提取，请稍候…</p></div>`}</div></section><section class="panel drama-shot-editor">${shot ? `<div class="panel-title"><div><h2>分镜编辑</h2><p>${rt().escapeHtml(shot.episode_name || '第1集')} · 当前第 ${shot.shot_index || 1} 条</p></div><button class="primary" id="drama-generate-shot-video">▣ 生成视频</button></div><label>分镜标题<input id="drama-shot-title" value="${rt().escapeHtml(shot.title)}" /></label><label>分镜文本<textarea id="drama-shot-original" rows="4">${rt().escapeHtml(shot.original_text)}</textarea></label><div class="drama-reference-panel"><div class="section-title"><div><h3>参考图</h3><p>从角色 / 场景 / 道具中选择当前分镜使用的素材。</p></div><div class="drama-quick-assets">${(['character', 'scene', 'prop'] as DramaAssetKind[]).map(kind => `<button class="ghost compact" data-drama-open-assets="${kind}">${dramaKindLabel(kind)} ${allAssets.filter(asset => asset.type === kind).length}</button>`).join('')}</div></div><div class="drama-reference-grid">${allAssets.slice(0, 6).map(asset => `<button class="drama-reference-card" data-drama-open-assets="${asset.type}"><span class="drama-reference-thumb">${asset.image_url ? `<img src="${rt().escapeHtml(resolveMediaUrl(asset.image_url))}" data-drama-image-preview="${rt().escapeHtml(resolveMediaUrl(asset.image_url))}" data-drama-image-label="${rt().escapeHtml(asset.name)}" alt="" />` : '＋'}</span><span><b>${rt().escapeHtml(asset.name)}</b><small>${dramaKindLabel(asset.type)} · ${rt().escapeHtml(asset.status)}</small></span></button>`).join('') || '<p class="muted">分镜拆解完成后，这里会出现角色、场景和道具。</p>'}</div></div><div class="drama-prompt-panel"><div class="section-title"><div><h3>提示词 <select class="drama-prompt-version" aria-label="提示词版本"><option>v1 · 多镜头</option></select></h3><p>按场景、角色、位置和连续镜头组织，素材会以图片引用胶囊形式展示。</p></div><div><button class="ghost compact" id="drama-generate-shot-prompt">✣ 生成提示词</button><button class="ghost compact" id="drama-copy-shot-prompt">▣ 复制</button></div></div><textarea id="drama-shot-prompt" data-drama-prompt-shot-id="${rt().escapeHtml(shot.id)}" rows="9" placeholder="点击生成提示词">${rt().escapeHtml(shot.prompt || '')}</textarea></div><div class="drama-params"><div><span>时长</span><strong>10s</strong></div><div><span>画幅</span><strong>${rt().escapeHtml(project.ratio || '9:16')}</strong></div><button class="ghost" id="drama-save-shot">保存分镜修改</button></div>` : `<div class="drama-generating large"><div class="empty-icon">◌</div><h3>分镜编辑</h3><p>等待剧本拆解完成后，选择一个分镜开始编辑。</p></div>`}</section><section class="panel drama-video-panel"><div class="panel-title"><div><h2>视频预览</h2><p>${shot ? `${history.length} 个历史版本` : '等待分镜'}</p></div><span class="status ${dramaStatusClass(shotStatus)}">${rt().escapeHtml(shotStatus)}</span></div>${selectedVideoSrc ? `<video id="drama-video-player" controls playsinline src="${rt().escapeHtml(selectedVideoSrc)}"></video>` : `<div class="drama-video-placeholder"><div>✦</div><strong>生成视频后将在这里预览</strong><span>每次生成都会保留历史版本，可在下方切换。</span></div>`}<div class="drama-video-history"><div class="section-title"><h3>视频历史</h3><span>${history.length} 个版本</span></div>${history.length ? history.map((video, index) => `<button class="drama-history-item ${video.url === selectedVideo ? 'selected' : ''}" data-drama-history-url="${rt().escapeHtml(video.url || '')}"><span>${video.url ? '▶' : '◌'}</span><div><b>版本 ${video.versionNo || history.length - index}</b><small>${dramaTime(video.createdAt)}</small></div></button>`).join('') : '<p class="muted">暂无历史视频</p>'}</div></section></div>${dramaViewState.assetPanel ? dramaAssetDrawer(project) : ''}</div>`;
}

export function dramaPromptNodes(project: ApiProject, shot: DramaShot): DramaPromptNode[] { const stored = Array.isArray(shot.prompt_rich) ? shot.prompt_rich : []; return placeTrailingDramaReferences(stored.length > 0 ? stored : shot.prompt ? [{ type: 'text', text: shot.prompt }] : [{ type: 'text', text: '' }]); }
export function serializeDramaPromptNodes(project: ApiProject, nodes: DramaPromptNode[]) { let mentionNumber = 0; const mentionNumbers = new Map<string, number>(); const normalized: DramaPromptNode[] = []; for (const node of nodes) { if (node.type === 'text') { if (node.text) normalized.push({ type: 'text', text: node.text }); continue; } const referenceNode = node as Extract<DramaPromptNode, { type: 'reference' }>; const asset = dramaReferenceAsset(dramaAssets(project), referenceNode); const assetType = referenceNode.asset_type || asset?.type || 'placeholder'; const assetKey = dramaReferenceKey(referenceNode) || `label:${referenceNode.label}`; if (!mentionNumbers.has(assetKey)) { mentionNumber += 1; mentionNumbers.set(assetKey, mentionNumber); } normalized.push({ type: 'reference', asset_id: referenceNode.asset_id, variant_id: referenceNode.variant_id || null, asset_type: assetType as DramaPromptAssetType, label: asset?.name || referenceNode.label || '占位图', image_url: asset?.image_url || referenceNode.image_url || null, mention_number: mentionNumbers.get(assetKey) }); } const prompt = normalized.map(node => node.type === 'text' ? node.text : `@图${node.mention_number}（${node.label}）`).join('').trim(); return { nodes: normalized, prompt }; }
export function renderDramaPromptNodes(root: HTMLElement, project: ApiProject, nodes: DramaPromptNode[]) { root.replaceChildren(); for (const node of nodes) { if (node.type === 'text') { root.appendChild(document.createTextNode(node.text)); continue; } const referenceNode = node as Extract<DramaPromptNode, { type: 'reference' }>; const chip = document.createElement('span'); chip.className = 'drama-prompt-reference'; chip.contentEditable = 'false'; chip.dataset.dramaPromptReference = 'true'; chip.dataset.assetId = referenceNode.asset_id; chip.dataset.variantId = referenceNode.variant_id || ''; chip.dataset.assetType = referenceNode.asset_type; chip.dataset.label = referenceNode.label; chip.dataset.imageUrl = referenceNode.image_url || ''; chip.dataset.mentionNumber = String(referenceNode.mention_number || 1); const asset = dramaReferenceAsset(dramaAssets(project), referenceNode); const imageUrl = resolveMediaUrl(asset?.image_url || referenceNode.image_url); if (dramaAssetImageIsGenerating(asset, project.tasks)) chip.innerHTML = dramaImageLoadingMarkup(referenceNode.label, rt().escapeHtml); else if (imageUrl) { const image = document.createElement('img'); image.src = imageUrl; image.alt = referenceNode.label; chip.appendChild(image); } else { const icon = document.createElement('span'); icon.className = 'drama-prompt-reference-placeholder'; icon.textContent = referenceNode.asset_type === 'character' ? '♙' : referenceNode.asset_type === 'scene' ? '✦' : referenceNode.asset_type === 'prop' ? '◆' : '＋'; chip.appendChild(icon); } const label = document.createElement('span'); label.textContent = `@图${referenceNode.mention_number || 1}（${referenceNode.label}）`; chip.appendChild(label); root.appendChild(chip); } }
export function readDramaPromptNodes(root: HTMLElement): DramaPromptNode[] { const nodes: DramaPromptNode[] = []; const visit = (node: Node) => { if (node.nodeType === Node.TEXT_NODE) { const text = node.textContent || ''; if (text) nodes.push({ type: 'text', text }); return; } if (!(node instanceof HTMLElement)) return; if (node.dataset.dramaPromptReference === 'true') { nodes.push({ type: 'reference', asset_id: node.dataset.assetId || '', variant_id: node.dataset.variantId || null, asset_type: (node.dataset.assetType || 'placeholder') as DramaPromptAssetType, label: node.dataset.label || '占位图', image_url: node.dataset.imageUrl || null, mention_number: Number(node.dataset.mentionNumber || 1) }); return; } if (node.tagName === 'BR') { nodes.push({ type: 'text', text: '\n' }); return; } node.childNodes.forEach(visit); }; root.childNodes.forEach(visit); return nodes; }
export function setupDramaRichPromptEditor(project: ApiProject, shot: DramaShot) { const textarea = document.querySelector<HTMLTextAreaElement>('#drama-shot-prompt'); if (!textarea || textarea.dataset.richEditorReady) return; textarea.dataset.richEditorReady = 'true'; const panel = textarea.closest('.drama-prompt-panel'); if (!panel) return; const toolbar = document.createElement('div'); toolbar.className = 'drama-rich-prompt-toolbar'; const toolbarLabel = document.createElement('span'); toolbarLabel.className = 'drama-rich-prompt-label'; toolbarLabel.textContent = '插入参考图：'; toolbar.appendChild(toolbarLabel); const editorFrame = document.createElement('div'); editorFrame.className = 'drama-rich-prompt-frame'; const editor = document.createElement('div'); editor.className = 'drama-rich-prompt-editor'; editor.contentEditable = 'true'; editor.setAttribute('role', 'textbox'); editor.setAttribute('aria-label', '分镜富文本提示词'); editorFrame.appendChild(editor); panel.insertBefore(toolbar, textarea); panel.insertBefore(editorFrame, textarea); textarea.hidden = true; textarea.classList.add('drama-rich-prompt-source'); textarea.style.display = 'none'; let nodes = dramaPromptNodes(project, shot); let savedRange: Range | null = null; const sync = () => { nodes = readDramaPromptNodes(editor); const serialized = serializeDramaPromptNodes(project, nodes); nodes = serialized.nodes; textarea.value = serialized.prompt; textarea.dataset.promptRich = JSON.stringify(serialized.nodes); editorFrame.classList.toggle('has-content', Boolean(serialized.prompt)); }; const rememberSelection = () => { const selection = window.getSelection(); if (!selection || selection.rangeCount === 0) return; const range = selection.getRangeAt(0); if (editor.contains(range.startContainer) && editor.contains(range.endContainer)) savedRange = range.cloneRange(); }; const insertNodeAtSelection = (node: DramaPromptNode) => { editor.focus(); const range = savedRange && editor.contains(savedRange.startContainer) && editor.contains(savedRange.endContainer) ? savedRange.cloneRange() : (() => { const fallback = document.createRange(); fallback.selectNodeContents(editor); fallback.collapse(false); return fallback; })(); range.deleteContents(); const temporary = document.createElement('span'); renderDramaPromptNodes(temporary, project, [node]); const chip = temporary.firstElementChild; if (!chip) return; range.insertNode(chip); const spacer = document.createTextNode(' '); range.setStartAfter(chip); range.collapse(true); range.insertNode(spacer); range.setStartAfter(spacer); range.collapse(true); const selection = window.getSelection(); selection?.removeAllRanges(); selection?.addRange(range); sync(); rememberSelection(); }; const addReferenceButton = (node: DramaPromptNode, buttonText: string) => { const button = document.createElement('button'); button.type = 'button'; button.className = 'drama-rich-prompt-reference-button'; button.textContent = buttonText; button.title = `插入${buttonText}`; button.addEventListener('mousedown', event => event.preventDefault()); button.addEventListener('click', () => insertNodeAtSelection(node)); toolbar.appendChild(button); }; (['character', 'scene', 'prop'] as DramaPromptAssetType[]).flatMap(type => dramaReferenceOptions(dramaAssets(project), type)).forEach(option => addReferenceButton(option.node, `${dramaKindLabel(option.asset.type)} · ${option.asset.name}`)); addReferenceButton({ type: 'reference', asset_id: `placeholder-${Date.now()}`, asset_type: 'placeholder', label: '占位图', image_url: null }, '占位图'); editor.addEventListener('input', sync); editor.addEventListener('mouseup', rememberSelection); editor.addEventListener('keyup', rememberSelection); editor.addEventListener('focus', rememberSelection); renderDramaPromptNodes(editor, project, nodes); sync(); }
export async function loadDramaProjects() { try { const response = await fetch(`${rt().apiBaseUrl}/projects`); if (!response.ok) return; const remote = await response.json() as ApiProject[]; rt().projects.splice(0, rt().projects.length, ...remote.map(project => rt().projectFromApi(project))); if (rt().active() === 'drama' && !document.querySelector('.drama-detail')) rt().render(); } catch (error) { console.warn('短剧列表加载失败', error); } }
export async function loadDramaDetail(id: string, retry = 0) { const main = document.querySelector('main'); if (!main) return; dramaViewState.projectId = id; try { await rt().loadVoicePresets(); const query = dramaViewState.shotId ? `?shot_id=${encodeURIComponent(dramaViewState.shotId)}` : ''; const response = await fetch(`${rt().apiBaseUrl}/projects/${id}${query}`); if (!response.ok) throw new Error(`HTTP ${response.status}`); const project = await response.json() as ApiProject; suppressExistingModelTaskFailureNotifications(project.tasks || []); main.innerHTML = dramaDetailMarkup(project); rt().bindDramaWorkspace(project); const shot = dramaSelectedShot(project); if (shot) setupDramaRichPromptEditor(project, shot); applyDramaGenerationLoading(project); scheduleDramaTaskRefresh(project); } catch (error) { rt().toast('短剧详情加载失败'); console.error(error); } }
export async function deleteDramaProject(projectId: string, fromDetail = false) {
  if (!await confirmAction({ title: '删除短剧？', description: '删除后，分镜、素材、任务和历史视频记录都会被永久删除，且无法恢复。', confirmLabel: '删除短剧' })) return;
  try {
    const response = await fetch(`${rt().apiBaseUrl}/projects/${projectId}`, { method: 'DELETE' });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const index = rt().projects.findIndex(project => project.id === projectId);
    if (index >= 0) rt().projects.splice(index, 1);
    if (fromDetail || dramaViewState.projectId === projectId) {
      dramaViewState.projectId = null;
      dramaViewState.shotId = null;
      dramaViewState.assetPanel = null;
      dramaViewState.videoUrl = null;
    }
    rt().toast('短剧及其全部资源已删除');
    rt().render();
    void rt().loadDramaProjects();
  } catch (error) {
    rt().toast('短剧删除失败，请稍后重试');
    console.error(error);
  }
}
export async function dramaPost(path: string, body?: unknown) {
  const response = await fetch(`${rt().apiBaseUrl}${path}`, { method: 'POST', ...(body === undefined ? {} : { headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }) });
  if (!response.ok) {
    const payload = await response.json().catch(() => ({})) as { detail?: unknown };
    throw new Error(typeof payload.detail === 'string' ? payload.detail : `HTTP ${response.status}`);
  }
  return await response.json() as GenerationTask;
}
export async function dramaRunTask(path: string, successMessage: string, body?: unknown) { try { const task = await dramaPost(path, body); if (successMessage) rt().toast(successMessage); if (task.warning_message?.trim()) rt().toast(task.warning_message); if (activeDramaProject && task?.id) { const updatedTypes = new Set([task.type]); activeDramaProject.tasks = [...(activeDramaProject.tasks || []).filter(item => item.id !== task.id), task]; activeDramaProject.assets?.forEach(asset => { if (asset.id === task.resource_id && ['asset_image', 'placeholder_image', 'cover_image'].includes(task.type)) asset.status = task.status; (asset.variants || []).forEach(variant => { if (variant.id === task.resource_id && task.type === 'asset_variant_image') variant.status = task.status; }); }); applyDramaGenerationLoading(activeDramaProject, updatedTypes); const sheet = document.querySelector<HTMLElement>('.drama-sheet-backdrop'); if (sheet && dramaViewState.assetPanel) replaceDramaAssetDrawer(sheet, dramaAssetDrawer(activeDramaProject), () => bindDramaAssetDrawer(activeDramaProject!), () => applyDramaGenerationLoading(activeDramaProject!, updatedTypes)); const editor = document.querySelector<HTMLElement>('.drama-rich-prompt-editor'); if (editor) renderDramaPromptNodes(editor, activeDramaProject, readDramaPromptNodes(editor)); scheduleDramaTaskRefresh(activeDramaProject); } return task; } catch (error) { rt().toast(`任务创建失败：${error instanceof Error ? error.message : '请检查素材是否已准备好'}`); console.error(error); return null; } }
export function dramaVideoPublicPrompt(project: ApiProject) { return project.video_public_prompt?.trim() || `整体保持${project.style || '真人风格'}，题材为${project.theme || '都市'}，按剧本处理方式组织镜头。\n视频全程保持画面内所有物体、道具、摆件数量不变，物体不消失、不凭空新增，物体位置轻微变化，物体形态材质保持一致，镜头平滑运动，无物体闪烁，无物体突然出现或突然消失，时序连贯，画面一致性强，流畅过渡`; }
const dramaAssetPromptLabels: Record<DramaAssetKind, string> = { character: '角色', scene: '场景', prop: '道具' };
export function dramaAssetPublicPromptDefault(project: ApiProject, kind: DramaAssetKind) { const style = project.style || '真人风格'; if (kind === 'character') return `图片风格为「${style}」，生成全身正视图以及一张面部特写（左边占二分之一的位置是超级大的正面面部特写，右边是二分之一放一张从头到鞋子的正视图，纯白背景，纯白背景）。`; if (kind === 'scene') return `图片风格为「${style}」，保持空间结构清晰、主体建筑或环境可识别，画面完整，适合作为短剧场景素材参考图。`; return `图片风格为「${style}」，主体道具清晰完整，材质、纹理和关键特征明确，画面干净，适合作为短剧道具素材参考图。`; }
export function dramaAssetPublicPrompt(project: ApiProject, kind: DramaAssetKind) { return project.asset_public_prompts?.[kind]?.trim() || dramaAssetPublicPromptDefault(project, kind); }
export function openAssetPublicPromptModal(project: ApiProject, kind: DramaAssetKind) { const label = dramaAssetPromptLabels[kind]; const defaultPrompt = dramaAssetPublicPromptDefault(project, kind); const modal = document.createElement('div'); modal.className = 'modal-backdrop asset-prompt-modal-backdrop'; modal.innerHTML = `<div class="modal video-prompt-modal asset-prompt-modal"><button class="close" aria-label="关闭">×</button><div class="modal-head"><h2>${label}公共提示词</h2><p>设置${label}图片生成时统一追加的公共提示词。</p></div><div class="video-prompt-body"><textarea id="asset-public-prompt-input" rows="4" autofocus>${rt().escapeHtml(dramaAssetPublicPrompt(project, kind))}</textarea></div><div class="video-prompt-actions"><button class="ghost" id="asset-public-prompt-default">↶&nbsp; 恢复默认</button><button class="ghost" id="asset-public-prompt-cancel">取消</button><button class="primary" id="asset-public-prompt-save">保存</button></div></div>`; document.body.append(modal); const close = () => modal.remove(); modal.querySelectorAll<HTMLElement>('.close,#asset-public-prompt-cancel').forEach(element => element.addEventListener('click', close)); modal.querySelector('#asset-public-prompt-default')?.addEventListener('click', () => { const input = modal.querySelector<HTMLTextAreaElement>('#asset-public-prompt-input'); if (input) input.value = defaultPrompt; }); modal.querySelector('#asset-public-prompt-save')?.addEventListener('click', async () => { const input = modal.querySelector<HTMLTextAreaElement>('#asset-public-prompt-input'); const button = modal.querySelector<HTMLButtonElement>('#asset-public-prompt-save'); if (!input || !button) return; button.disabled = true; try { const response = await fetch(`${rt().apiBaseUrl}/projects/${project.id}/asset-public-prompt`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ asset_type: kind, public_prompt: input.value }) }); if (!response.ok) throw new Error(`HTTP ${response.status}`); rt().toast(`${label}公共提示词已保存`); close(); void rt().loadDramaDetail(project.id); } catch (error) { button.disabled = false; rt().toast(`${label}公共提示词保存失败`); console.error(error); } }); modal.querySelector<HTMLTextAreaElement>('#asset-public-prompt-input')?.focus(); }
export function openDramaAssetEditorModal(project: ApiProject, kind: DramaAssetKind, asset?: DramaAsset) {
  const editing = Boolean(asset);
  const label = dramaKindLabel(kind);
  const selectedVoice = rt().voicePreset(asset?.voice_id);
  const voiceField = kind === 'character' ? `<label>角色音色<select id="drama-asset-voice">${rt().voiceOptions(asset?.voice_id)}</select><small class="drama-voice-help">${rt().escapeHtml(selectedVoice?.prompt || '不设置音色')}</small></label>` : '';
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop';
  modal.innerHTML = `<div class="modal drama-asset-editor-modal"><button class="close" aria-label="关闭">×</button><div class="modal-head"><h2>${editing ? `编辑${label}` : `添加${label}`}</h2><p>${editing ? '修改素材名称和图片提示词。' : `新增一个${label}，保存后可以直接生成图片。`}</p></div><label>${label}名称<input id="drama-asset-name" value="${rt().escapeHtml(asset?.name || '')}" placeholder="例如：男主、青云山修仙者" /></label>${voiceField}<label>图片提示词<textarea id="drama-asset-prompt" rows="6" placeholder="描述外观、材质、关键特征和需要保持一致的细节">${rt().escapeHtml(asset?.prompt || '')}</textarea></label><div class="modal-actions"><button class="ghost" id="drama-asset-cancel">取消</button><button class="primary" id="drama-asset-save">${editing ? '保存修改' : `添加${label}`}</button></div></div>`;
  document.body.append(modal);
  const close = () => modal.remove();
  modal.querySelectorAll<HTMLElement>('.close,#drama-asset-cancel').forEach(element => element.addEventListener('click', close));
  modal.querySelector('#drama-asset-save')?.addEventListener('click', async () => {
    const name = (modal.querySelector('#drama-asset-name') as HTMLInputElement).value.trim();
    const prompt = (modal.querySelector('#drama-asset-prompt') as HTMLTextAreaElement).value.trim();
    const voiceId = kind === 'character' ? (modal.querySelector<HTMLSelectElement>('#drama-asset-voice')?.value || '') : '';
    const voicePayload = kind === 'character' ? { voice_id: voiceId || null } : {};
    const button = modal.querySelector<HTMLButtonElement>('#drama-asset-save');
    if (!name) { rt().toast(`请填写${label}名称`); return; }
    if (button) { button.disabled = true; button.textContent = '保存中…'; }
    try {
      const response = await fetch(editing ? `${rt().apiBaseUrl}/projects/${project.id}/assets/${asset!.id}` : `${rt().apiBaseUrl}/projects/${project.id}/assets`, { method: editing ? 'PUT' : 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(editing ? { name, prompt, ...voicePayload } : { type: kind, name, prompt, ...voicePayload }) });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      rt().toast(editing ? `${label}已保存` : `${label}已添加`);
      close();
      void rt().loadDramaDetail(project.id);
    } catch (error) { if (button) { button.disabled = false; button.textContent = editing ? '保存修改' : `添加${label}`; } rt().toast(`${label}保存失败`); console.error(error); }
  });
  modal.querySelector<HTMLSelectElement>('#drama-asset-voice')?.addEventListener('change', event => { const help = modal.querySelector<HTMLElement>('.drama-voice-help'); if (help) help.textContent = rt().voicePreset((event.target as HTMLSelectElement).value)?.prompt || '不设置音色'; }); modal.querySelector<HTMLInputElement>('#drama-asset-name')?.focus();
}

export function openDramaVariantModal(project: ApiProject, asset: DramaAsset, variant?: DramaAssetVariant, outfit = false) {
  const editing = Boolean(variant);
  const copy = asset.type === 'character'
    ? { description: '保持人物的脸部、体态和基础特征一致，只改变服装或外观设定。', placeholder: '例如：青云山道袍、战斗形态，描述服装、发型、动作或外观变化。' }
    : asset.type === 'scene'
      ? { description: '保持场景的空间结构和主要特征一致，只改变时间、天气、陈设或环境状态。', placeholder: '例如：雨夜状态、战后废墟，描述时间、天气、陈设或环境变化。' }
      : { description: '保持道具的主体特征和材质一致，只改变使用状态、破损程度或外观细节。', placeholder: '例如：被使用过的状态、破损形态，描述材质、状态或外观变化。' };
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop';
  modal.innerHTML = `<div class="modal drama-asset-editor-modal"><button class="close" aria-label="关闭">×</button><div class="modal-head"><h2>${editing ? '编辑形态' : outfit ? '换装' : '添加其他形态'}</h2><p>${copy.description}</p></div><label>形态名称<input id="drama-variant-name" value="${rt().escapeHtml(variant?.name || (outfit ? '新换装' : '其他形态'))}" placeholder="例如：基础形态、雨夜状态、战斗形态" /></label><label>形态图片提示词<textarea id="drama-variant-prompt" rows="6" placeholder="${copy.placeholder}">${rt().escapeHtml(variant?.prompt || (outfit ? '保持角色面部特征和体态一致，换穿符合剧情的全新服装。' : ''))}</textarea></label><div class="modal-actions"><button class="ghost" id="drama-variant-cancel">取消</button><button class="primary" id="drama-variant-save">${editing ? '保存修改' : '添加形态'}</button></div></div>`;
  document.body.append(modal);
  const close = () => modal.remove();
  modal.querySelectorAll<HTMLElement>('.close,#drama-variant-cancel').forEach(element => element.addEventListener('click', close));
  modal.querySelector('#drama-variant-save')?.addEventListener('click', async () => {
    const name = (modal.querySelector('#drama-variant-name') as HTMLInputElement).value.trim();
    const prompt = (modal.querySelector('#drama-variant-prompt') as HTMLTextAreaElement).value.trim();
    const button = modal.querySelector<HTMLButtonElement>('#drama-variant-save');
    if (!name) { rt().toast('请填写形态名称'); return; }
    if (button) { button.disabled = true; button.textContent = '保存中…'; }
    try {
      const url = editing ? `${rt().apiBaseUrl}/projects/${project.id}/assets/${asset.id}/variants/${variant!.id}` : `${rt().apiBaseUrl}/projects/${project.id}/assets/${asset.id}/variants`;
      const response = await fetch(url, { method: editing ? 'PUT' : 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, prompt }) });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      rt().toast(editing ? '形态已保存' : '形态已添加');
      close();
      void rt().loadDramaDetail(project.id);
    } catch (error) { if (button) { button.disabled = false; button.textContent = editing ? '保存修改' : '添加形态'; } rt().toast('形态保存失败'); console.error(error); }
  });
  modal.querySelector<HTMLInputElement>('#drama-variant-name')?.focus();
}

export function uploadDramaAssetImage(project: ApiProject, asset: DramaAsset) {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = 'image/png,image/jpeg,image/webp,image/gif';
  input.addEventListener('change', () => {
    const file = input.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = async () => {
      try {
        const response = await fetch(`${rt().apiBaseUrl}/projects/${project.id}/assets/${asset.id}/upload`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ data_url: String(reader.result || '') }) });
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        rt().toast(`${dramaKindLabel(asset.type)}图片已上传`);
        void rt().loadDramaDetail(project.id);
      } catch (error) { rt().toast('图片上传失败'); console.error(error); }
    };
    reader.readAsDataURL(file);
  });
  input.click();
}

export const openDramaImageHistoryModal = (asset: DramaAsset | DramaAssetVariant, label: string) => renderDramaImageHistoryModal(asset, label, resolveMediaUrl, rt().escapeHtml, dramaTime);

export function openDramaFrameModal(project: ApiProject) {
  const shot = dramaSelectedShot(project);
  if (!shot) return rt().toast('请先选择一个分镜');
  const frames = shot.first_last_frames || {};
  const episodeShots = (project.shots || []).filter(item => item.episode_id === shot.episode_id);
  const choices = (side: 'first' | 'last') => episodeShots.flatMap(item => {
    const positions = ['first', 'last'] as const;
    const versionVideos = (item.versions || []).filter(version => version.status === '生成成功' && version.video_url).flatMap(version => positions.map(position => ({ url: version.video_url || '', source: 'video', shotId: item.id, videoId: version.id, label: `分镜${item.shot_index || 1}-V${version.version_no || 1}`, position })));
    const knownVideoUrls = new Set(versionVideos.map(video => video.url));
    const legacyVideos = (item.historical_videos || []).filter(video => video.url && !knownVideoUrls.has(video.url)).flatMap((video, index) => positions.map(position => ({ url: video.url || '', source: 'video', shotId: item.id, videoId: video.id || String(index), label: `分镜${item.shot_index || 1}-V${index + 1}`, position })));
    const refs = positions.flatMap(position => { const bound = item.first_last_frames?.[position]; return bound?.url ? [{ url: resolveDramaFrameUrl(bound.url), source: bound.source || 'frame', shotId: item.id, videoId: bound.video_id || '', label: `分镜 ${item.shot_index || ''} · 已绑定${position === 'first' ? '首帧' : '尾帧'}`, position }] : []; });
    return [...refs, ...versionVideos, ...legacyVideos];
  });
  const isChoiceSelected = (side: 'first' | 'last', choice: { shotId: string; videoId: string; position: 'first' | 'last' }) => { const frame = frames[side]; return Boolean(frame?.shot_id === choice.shotId && frame?.video_id === choice.videoId && (frame?.position || side) === choice.position); };
  const sourcePanel = (side: 'first' | 'last') => `<div class="drama-frame-source-panel" data-frame-source-panel="${side}" hidden><div class="drama-frame-source-head"><div><h4>选择输入${side === 'first' ? '首' : '尾'}帧</h4><p>当前集内其他分镜的首帧和尾帧均可选择</p></div><button type="button" class="drama-frame-source-close" data-frame-library-close="${side}" aria-label="关闭">×</button></div><div class="drama-frame-source-tabs"><button type="button" class="active" data-frame-tab="${side}-library">▧ 可用首尾帧 <b>${choices(side).filter(choice => choice.source !== 'video').length}</b></button><button type="button" data-frame-tab="${side}-video">▣ 从视频提取 <b>${choices(side).filter(choice => choice.source === 'video').length}</b></button></div><div class="drama-frame-choice-grid" data-frame-choice-grid="${side}">${choices(side).map(choice => `<button type="button" class="drama-frame-choice-card ${isChoiceSelected(side, choice) ? 'selected' : ''}" data-frame-choice="${side}" data-frame-url="${rt().escapeHtml(choice.url)}" data-frame-source="${choice.source}" data-frame-shot="${choice.shotId}" data-frame-video="${choice.videoId}" data-frame-position="${choice.position}" data-frame-choice-kind="${choice.source === 'video' ? 'video' : 'library'}"><span class="drama-frame-choice-thumb" ${choice.source === 'video' ? `data-frame-thumb-url="${rt().escapeHtml(choice.url)}" data-frame-thumb-side="${choice.position}"><span class="drama-frame-ai-badge">AI生成</span><span>提取中…</span>` : `><span class="drama-frame-ai-badge">AI生成</span><img src="${rt().escapeHtml(resolveMediaUrl(choice.url))}" alt="" />`}</span><span class="drama-frame-choice-label">${rt().escapeHtml(choice.label)}<small>${choice.source === 'video' ? `取视频${choice.position === 'first' ? '首' : '尾'}帧` : '已绑定帧'}</small></span></button>`).join('') || '<small>本集暂无可用素材</small>'}</div></div>`;
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop';
  modal.innerHTML = `<div class="modal drama-frame-modal"><button type="button" class="close">×</button><div class="modal-head"><h2>首尾帧 · ${rt().escapeHtml(shot.episode_name || '当前分镜')} / 分镜 ${shot.shot_index || 1}</h2><p>首尾帧只在当前分镜所属集内复用，用于连接相邻分镜。</p></div><div class="drama-frame-editor-grid">${(['first', 'last'] as const).map(side => `<section class="drama-frame-editor-card"><h3>${side === 'first' ? '输入首帧' : '输入尾帧'}</h3><div class="drama-frame-preview">${frames[side]?.url ? `<span class="drama-frame-ai-badge">AI生成</span><img src="${rt().escapeHtml(resolveDramaFrameUrl(frames[side]?.url || ''))}" alt="" />` : '尚未设置'}</div><div class="drama-frame-actions"><button type="button" class="ghost compact" data-frame-library="${side}">▧ 从${side === 'first' ? '首' : '尾'}帧库选择</button><label class="ghost compact">↥ 上传图片<input type="file" accept="image/*" data-frame-upload="${side}" hidden /></label></div>${sourcePanel(side)}</section>`).join('')}</div><div class="modal-actions"><button type="button" class="ghost" data-frame-clear>清除首尾帧</button><button type="button" class="primary" data-frame-save>完成</button></div></div>`;
  document.body.append(modal);
  const frameSubtitle = modal.querySelector<HTMLElement>('.modal-head p');
  if (frameSubtitle) frameSubtitle.textContent = '首尾帧会作为普通参考图与当前分镜素材一起发送，并由提示词约束视频的起止画面。';
  const values: Record<string, unknown> = { first: frames.first || null, last: frames.last || null };
  modal.querySelectorAll<HTMLElement>('[data-frame-library]').forEach(button => button.addEventListener('click', () => { const panel = modal.querySelector<HTMLElement>(`[data-frame-source-panel="${button.dataset.frameLibrary}"]`); if (panel) panel.hidden = !panel.hidden; }));
  modal.querySelectorAll<HTMLElement>('[data-frame-library-close]').forEach(button => button.addEventListener('click', () => { const panel = modal.querySelector<HTMLElement>(`[data-frame-source-panel="${button.dataset.frameLibraryClose}"]`); if (panel) panel.hidden = true; }));
  modal.querySelectorAll<HTMLElement>('[data-frame-tab]').forEach(button => button.addEventListener('click', () => { const tab = button.dataset.frameTab || ''; const side = tab.startsWith('first') ? 'first' : 'last'; const kind = tab.endsWith('video') ? 'video' : 'library'; modal.querySelectorAll(`[data-frame-tab^="${side}-"]`).forEach(item => item.classList.toggle('active', item === button)); modal.querySelectorAll<HTMLElement>(`[data-frame-choice][data-frame-choice-kind]`).forEach(item => { if ((item.dataset.frameChoice || '') === side) item.hidden = item.dataset.frameChoiceKind !== kind; }); }));
  modal.querySelectorAll<HTMLElement>('[data-frame-thumb-url]').forEach(async thumb => { const image = await captureDramaVideoFrame(thumb.dataset.frameThumbUrl || '', thumb.dataset.frameThumbSide as 'first' | 'last', resolveMediaUrl); if (image) thumb.innerHTML = `<span class="drama-frame-ai-badge">AI生成</span><img src="${image}" alt="" />`; else thumb.textContent = '无法提取'; });
  modal.querySelectorAll<HTMLElement>('[data-frame-choice]').forEach(button => button.addEventListener('click', async () => { const side = (button.dataset.frameChoice || 'first') as 'first' | 'last'; const position = (button.dataset.framePosition || side) as 'first' | 'last'; const preview = button.closest('section')?.querySelector('.drama-frame-preview'); if (!preview) return; const image = button.querySelector<HTMLImageElement>('.drama-frame-choice-thumb img')?.src || await captureDramaVideoFrame(button.dataset.frameUrl || '', position, resolveMediaUrl); if (!image) { preview.textContent = '无法提取帧'; return; } values[side] = { url: image, source: 'frame', shot_id: button.dataset.frameShot, video_id: button.dataset.frameVideo, position }; modal.querySelectorAll<HTMLElement>(`[data-frame-choice="${side}"]`).forEach(item => item.classList.toggle('selected', item === button)); preview.innerHTML = `<span class="drama-frame-ai-badge">AI生成</span><img src="${image}" alt="${side === 'first' ? '首帧' : '尾帧'}" />`; }));
  modal.querySelectorAll<HTMLInputElement>('[data-frame-upload]').forEach(input => input.addEventListener('change', () => { const file = input.files?.[0]; if (!file) return; const reader = new FileReader(); reader.onload = () => { const side = input.dataset.frameUpload || 'first'; const url = String(reader.result || ''); values[side] = { url, source: 'upload' }; modal.querySelectorAll<HTMLElement>(`[data-frame-choice="${side}"]`).forEach(item => item.classList.remove('selected')); const preview = input.closest('section')?.querySelector('.drama-frame-preview'); if (preview) preview.innerHTML = `<img src="${url}" alt="" />`; }; reader.readAsDataURL(file); }));
  modal.querySelector('[data-frame-clear]')?.addEventListener('click', () => { values.first = null; values.last = null; modal.querySelectorAll('.drama-frame-preview').forEach(item => item.textContent = '尚未设置'); modal.querySelectorAll('.drama-frame-choice-card.selected').forEach(item => item.classList.remove('selected')); });
  modal.querySelector('[data-frame-save]')?.addEventListener('click', async event => {
    event.preventDefault(); const button = event.currentTarget as HTMLButtonElement; if (button.disabled) return; button.disabled = true; button.textContent = '保存中…';
    try { await flushDramaEditorAutosave(); const response = await fetch(`${rt().apiBaseUrl}/projects/${project.id}/shots/${shot.id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ first_last_frames: values }) }); if (!response.ok) throw new Error(`HTTP ${response.status}`); modal.remove(); rt().toast('首尾帧已保存'); void rt().loadDramaDetail(project.id); }
    catch (error) { button.disabled = false; button.textContent = '完成'; rt().toast('首尾帧保存失败，请稍后重试'); console.error(error); }
  });
  modal.querySelector('.close')?.addEventListener('click', () => modal.remove());
}
export function bindDramaAssetDrawer(project: ApiProject) { document.querySelectorAll<HTMLButtonElement>('[data-drama-generate-asset]').forEach(button => button.addEventListener('click', event => { event.preventDefault(); event.stopImmediatePropagation(); void dramaRunTask(`/projects/${project.id}/assets/${button.dataset.dramaGenerateAsset}/image`, '素材图片任务已创建'); })); bindDramaEpisodeManager(project); document.querySelector<HTMLElement>('.drama-asset-sheet')?.addEventListener('click', event => event.stopPropagation()); document.querySelectorAll<HTMLButtonElement>('[data-drama-asset-tab]').forEach(button => button.addEventListener('click', event => { const kind = button.dataset.dramaAssetTab as DramaAssetKind; if (!['character', 'scene', 'prop'].includes(kind)) return; event.preventDefault(); event.stopImmediatePropagation(); dramaViewState.assetPanel = kind; const drawer = document.querySelector<HTMLElement>('.drama-sheet-backdrop'); if (!drawer || !activeDramaProject) return; const wrapper = document.createElement('div'); wrapper.innerHTML = dramaAssetDrawer(activeDramaProject); drawer.replaceWith(wrapper.firstElementChild as HTMLElement); bindDramaAssetDrawer(activeDramaProject); applyDramaGenerationLoading(activeDramaProject); }));
  document.querySelector('[data-drama-generate-all-assets]')?.addEventListener('click', event => {
    event.preventDefault();
    event.stopImmediatePropagation();
    const assets = dramaAssets(project).filter(asset => asset.type === dramaViewState.assetPanel);
    void dramaRunTask(
      `/projects/${project.id}/assets/images/batch`,
      `已开始按每批 5 张生成 ${assets.length} 个${dramaKindLabel(dramaViewState.assetPanel || 'prop')}素材`,
      { asset_ids: assets.map(asset => asset.id) },
    );
  });
  document.querySelectorAll<HTMLElement>('[data-drama-add-asset]').forEach(element => element.addEventListener('click', event => { event.preventDefault(); event.stopImmediatePropagation(); openDramaAssetEditorModal(project, (element.dataset.dramaAddAsset || 'prop') as DramaAssetKind); }));
  document.querySelectorAll<HTMLElement>('[data-drama-edit-asset]').forEach(element => element.addEventListener('click', event => { event.preventDefault(); event.stopImmediatePropagation(); const asset = dramaAssets(project).find(item => item.id === element.dataset.dramaEditAsset); if (asset) openDramaAssetEditorModal(project, asset.type as DramaAssetKind, asset); }));
  document.querySelectorAll<HTMLElement>('[data-drama-delete-asset]').forEach(element => element.addEventListener('click', async event => { event.preventDefault(); event.stopImmediatePropagation(); const asset = dramaAssets(project).find(item => item.id === element.dataset.dramaDeleteAsset); if (!asset) return; if (!await confirmAction({ title: '删除素材？', description: `确认删除${dramaKindLabel(asset.type)}“${asset.name}”？此操作无法恢复。`, confirmLabel: '删除素材' })) return; const response = await fetch(`${rt().apiBaseUrl}/projects/${project.id}/assets/${asset.id}`, { method: 'DELETE' }); if (response.ok) { rt().toast(`${dramaKindLabel(asset.type)}已删除`); void rt().loadDramaDetail(project.id); } else rt().toast('删除失败'); }));
  document.querySelectorAll<HTMLElement>('[data-drama-upload-asset]').forEach(element => element.addEventListener('click', event => { event.preventDefault(); event.stopImmediatePropagation(); const asset = dramaAssets(project).find(item => item.id === element.dataset.dramaUploadAsset); if (asset) uploadDramaAssetImage(project, asset); }));
  document.querySelectorAll<HTMLElement>('[data-drama-add-variant],[data-drama-change-outfit]').forEach(element => element.addEventListener('click', event => { event.preventDefault(); event.stopImmediatePropagation(); const asset = dramaAssets(project).find(item => item.id === (element.dataset.dramaAddVariant || element.dataset.dramaChangeOutfit)); if (asset) openDramaVariantModal(project, asset, undefined, Boolean(element.dataset.dramaChangeOutfit)); }));
  document.querySelectorAll<HTMLElement>('[data-drama-edit-variant]').forEach(element => element.addEventListener('click', event => { event.preventDefault(); event.stopImmediatePropagation(); const asset = dramaAssets(project).find(item => item.id === element.dataset.dramaEditVariant); const variant = asset?.variants?.find(item => item.id === element.dataset.dramaVariantId); if (asset && variant) openDramaVariantModal(project, asset, variant); }));
  document.querySelectorAll<HTMLElement>('[data-drama-delete-variant]').forEach(element => element.addEventListener('click', async event => { event.preventDefault(); event.stopImmediatePropagation(); const asset = dramaAssets(project).find(item => item.id === element.dataset.dramaDeleteVariant); const variant = asset?.variants?.find(item => item.id === element.dataset.dramaVariantId); if (!asset || !variant) return; if (!await confirmAction({ title: '删除形态？', description: `确认删除形态“${variant.name}”？此操作无法恢复。`, confirmLabel: '删除形态' })) return; const response = await fetch(`${rt().apiBaseUrl}/projects/${project.id}/assets/${asset.id}/variants/${variant.id}`, { method: 'DELETE' }); if (response.ok) { rt().toast('形态已删除'); void rt().loadDramaDetail(project.id); } else rt().toast('形态删除失败'); }));
  document.querySelectorAll<HTMLElement>('[data-drama-image-history]').forEach(element => element.addEventListener('click', event => { event.preventDefault(); event.stopImmediatePropagation(); const parent = dramaAssets(project).find(item => item.id === element.dataset.dramaParentAsset); const item = parent?.id === element.dataset.dramaImageHistory ? parent : parent?.variants?.find(variant => variant.id === element.dataset.dramaImageHistory); if (item) openDramaImageHistoryModal(item, item === parent ? item.name : `${parent?.name || '角色'} · ${item.name}`); }));
  document.querySelector('[data-drama-collapse-assets]')?.addEventListener('click', () => { dramaViewState.assetPanel = null; void rt().loadDramaDetail(project.id); });
  document.querySelector('[data-drama-toggle-search]')?.addEventListener('click', () => { const search = document.querySelector<HTMLElement>('.drama-asset-search'); if (search) search.hidden = !search.hidden; search?.querySelector<HTMLInputElement>('input')?.focus(); });
  document.querySelector<HTMLInputElement>('[data-drama-asset-search]')?.addEventListener('input', event => { const query = (event.target as HTMLInputElement).value.trim().toLowerCase(); document.querySelectorAll<HTMLElement>('[data-drama-asset-card]').forEach(card => { card.hidden = Boolean(query) && !(card.dataset.assetName || '').includes(query); }); });
  document.querySelector('[data-drama-toggle-filter]')?.addEventListener('click', () => rt().toast('筛选功能将在素材状态更多时开放'));
  document.querySelectorAll<HTMLElement>('[data-drama-generate-variant]').forEach(element => element.addEventListener('click', () => void dramaRunTask(`/projects/${project.id}/assets/${element.dataset.dramaGenerateVariant}/variants/${element.dataset.dramaVariantId}/image`, '形态图片任务已创建')));
}
