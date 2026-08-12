/** ZIP download dialog for assembling each episode from creator-selected video history versions. */
import './drama_video_export.css';

import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import { activeDramaProject } from './drama_state.js';
import { dramaVideoHistoryRecords, type DramaVideoHistoryRecord } from './drama_video_history.js';
import { icon } from './ui_icons.js';
import type { ApiProject, DramaShot, GenerationTask } from './models.js';

type DramaVideoExportRuntime = {
  apiBaseUrl: string;
  escapeHtml: (value: unknown) => string;
  resolveMediaUrl: (value?: string | null) => string;
  toast: (message: string) => void;
};

type ExportDialogState = {
  project: ApiProject;
  taskId?: string;
  timer?: number;
};

let runtime: DramaVideoExportRuntime | null = null;
let state: ExportDialogState | null = null;

function rt() {
  if (!runtime) throw new Error('视频导出界面尚未初始化');
  return runtime;
}

function playableVersions(shot: DramaShot) {
  return dramaVideoHistoryRecords(shot).filter(record => record.status === '生成成功' && Boolean(record.url));
}

function defaultVersion(versions: DramaVideoHistoryRecord[]) {
  return versions.find(record => record.selectedForExport) || versions[0];
}

function versionOptions(versions: DramaVideoHistoryRecord[]) {
  if (!versions.length) return '<option value="" selected>暂无已生成视频</option>';
  const selected = defaultVersion(versions)?.id;
  const hasSelectedForExport = versions.some(record => record.selectedForExport);
  return versions.map((record, index) => {
    const label = record.selectedForExport
      ? '（使用版本）'
      : !hasSelectedForExport && index === 0
        ? '（最新版本）'
        : '';
    return `<option value="${rt().escapeHtml(record.id)}"${record.id === selected ? ' selected' : ''}>v${record.versionNo || '—'}${label}</option>`;
  }).join('');
}

function shotLabel(shot: DramaShot, index: number) {
  const episode = shot.episode_name || '第1集';
  return `${episode} · 分镜 ${shot.shot_index || index + 1} · ${shot.title || '未命名分镜'}`;
}

function selectionsMarkup(project: ApiProject) {
  const shots = project.shots || [];
  if (!shots.length) return '<p class="drama-video-export-empty">当前还没有分镜视频可打包。</p>';
  return shots.map((shot, index) => {
    const versions = playableVersions(shot);
    const label = shotLabel(shot, index);
    return `<div class="drama-video-export-shot${versions.length ? '' : ' missing'}"><span class="drama-video-export-shot-title">${rt().escapeHtml(label)}</span><select aria-label="选择${rt().escapeHtml(label)}的使用版本" data-video-export-version="${rt().escapeHtml(shot.id)}"${versions.length ? '' : ' disabled'}>${versionOptions(versions)}</select></div>`;
  }).join('');
}

function openModal(project: ApiProject) {
  closeModal();
  state = { project };
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop drama-video-export-backdrop';
  modal.dataset.dramaVideoExportBackdrop = 'true';
  modal.innerHTML = `<section class="modal drama-video-export-modal" role="dialog" aria-modal="true" aria-labelledby="drama-video-export-title"><header class="drama-video-export-head"><div><h2 id="drama-video-export-title">打包下载视频</h2><p>每集会按已选择分镜的顺序拼接为一个 MP4 文件，再统一压缩为 ZIP。</p></div><button type="button" class="close" data-video-export-close aria-label="关闭">×</button></header><div class="drama-video-export-form"><div class="drama-video-export-format"><span class="drama-video-export-format-label">导出格式</span><span class="drama-video-export-format-value">MP4 视频</span></div><div class="drama-video-export-selection-head"><div><h3>分镜使用版本</h3><p>默认选择“视频历史”中标记的使用版本；未生成视频的分镜会自动跳过。</p></div><span>${(project.shots || []).length} 条分镜</span></div><div class="drama-video-export-selections">${selectionsMarkup(project)}</div><p class="drama-video-export-error" data-video-export-error hidden></p></div><div class="drama-video-export-progress" data-video-export-progress hidden><div class="drama-video-export-progress-copy"><strong>正在准备下载</strong><span data-video-export-stage>等待任务开始</span></div><progress value="0" max="100" data-video-export-progress-value></progress><span data-video-export-progress-label>0%</span></div><footer class="modal-actions drama-video-export-actions"><button type="button" class="ghost" data-video-export-cancel>取消</button><button type="button" class="primary" data-video-export-start>${icon('download')}<span>下载 ZIP</span></button></footer></section>`;
  document.body.append(modal);
  modal.querySelectorAll<HTMLElement>('[data-video-export-close]').forEach(button => button.addEventListener('click', closeModal));
  modal.querySelector<HTMLButtonElement>('[data-video-export-cancel]')?.addEventListener('click', () => void cancelOrClose());
  modal.querySelector<HTMLButtonElement>('[data-video-export-start]')?.addEventListener('click', () => void startExport());
  modal.addEventListener('click', event => { if (event.target === modal) closeModal(); });
}

function modal() {
  return document.querySelector<HTMLElement>('[data-drama-video-export-backdrop]');
}

function closeModal() {
  if (state?.timer !== undefined) window.clearTimeout(state.timer);
  state = null;
  modal()?.remove();
}

function selectedVersions() {
  return [...(modal()?.querySelectorAll<HTMLSelectElement>('[data-video-export-version]') || [])]
    .map(select => ({ shot_id: select.dataset.videoExportVersion || '', version_id: select.value }))
    .filter(item => Boolean(item.shot_id && item.version_id));
}

function setError(message = '') {
  const target = modal()?.querySelector<HTMLElement>('[data-video-export-error]');
  if (!target) return;
  target.hidden = !message;
  target.textContent = message;
}

async function startExport() {
  if (!state) return;
  const selections = selectedVersions();
  if (!selections.length) {
    setError('请至少选择一个已生成的视频版本。');
    return;
  }
  const start = modal()?.querySelector<HTMLButtonElement>('[data-video-export-start]');
  if (start) { start.disabled = true; start.textContent = '正在创建下载任务…'; }
  setError();
  try {
    const response = await fetch(`${rt().apiBaseUrl}/projects/${encodeURIComponent(state.project.id)}/video-exports`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ format: 'mp4', selections }),
    });
    const task = await response.json().catch(() => ({})) as GenerationTask & { detail?: string };
    if (!response.ok) throw new Error(task.detail || `HTTP ${response.status}`);
    state.taskId = task.id;
    showProgress(task);
    pollExportTask();
  } catch (error) {
    if (start) { start.disabled = false; start.innerHTML = `${icon('download')}<span>下载 ZIP</span>`; }
    setError(error instanceof Error ? error.message : '创建视频下载任务失败');
  }
}

function showProgress(task: GenerationTask) {
  const dialog = modal();
  if (!dialog) return;
  dialog.querySelector<HTMLElement>('.drama-video-export-form')!.hidden = true;
  dialog.querySelector<HTMLElement>('[data-video-export-progress]')!.hidden = false;
  dialog.querySelector<HTMLButtonElement>('[data-video-export-start]')!.hidden = true;
  const progress = Math.max(0, Math.min(100, Number(task.progress || 0)));
  dialog.querySelector<HTMLProgressElement>('[data-video-export-progress-value]')!.value = progress;
  dialog.querySelector<HTMLElement>('[data-video-export-progress-label]')!.textContent = `${progress}%`;
  dialog.querySelector<HTMLElement>('[data-video-export-stage]')!.textContent = task.stage || '正在处理…';
}

async function pollExportTask() {
  if (!state?.taskId) return;
  const taskId = state.taskId;
  try {
    const response = await fetch(`${rt().apiBaseUrl}/projects/${encodeURIComponent(state.project.id)}/video-exports/${encodeURIComponent(taskId)}`);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const task = await response.json() as GenerationTask;
    showProgress(task);
    if (task.status === '生成中') {
      state.timer = window.setTimeout(() => void pollExportTask(), 700);
      return;
    }
    if (task.status === '生成成功') {
      const result = task.result as { url?: string; file_name?: string } | null;
      if (!result?.url) throw new Error('ZIP 已完成但找不到下载文件');
      if (state) state.taskId = undefined;
      await completeDownload(taskId, result.url, result.file_name || '短剧视频合集.zip');
      return;
    }
    throw new Error(task.status === '已取消' ? '视频打包已取消' : task.error_message || '视频打包失败');
  } catch (error) {
    const message = error instanceof Error ? error.message : '读取下载进度失败';
    if (state) state.taskId = undefined;
    const dialog = modal();
    dialog?.querySelector<HTMLElement>('[data-video-export-progress]')?.setAttribute('hidden', '');
    const form = dialog?.querySelector<HTMLElement>('.drama-video-export-form');
    if (form) form.hidden = false;
    const start = dialog?.querySelector<HTMLButtonElement>('[data-video-export-start]');
    if (start) { start.hidden = false; start.disabled = false; start.innerHTML = `${icon('download')}<span>重新下载 ZIP</span>`; }
    setError(message);
  }
}

function isDesktopApp() {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

function saveActionButton(dialog: HTMLElement, fileName: string, onClick: () => void) {
  const existing = dialog.querySelector<HTMLButtonElement>('[data-video-export-save]');
  if (existing) return existing;
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'ghost drama-video-export-save';
  button.dataset.videoExportSave = 'true';
  button.innerHTML = `${icon('download')}<span>选择位置保存 ZIP</span>`;
  button.title = `选择保存位置：${fileName}`;
  button.addEventListener('click', onClick);
  dialog.querySelector<HTMLElement>('[data-video-export-progress]')?.append(button);
  return button;
}

async function saveDesktopZip(taskId: string, fileName: string) {
  if (!state) return;
  const dialog = modal();
  if (!dialog) return;
  const stage = dialog.querySelector<HTMLElement>('[data-video-export-stage]');
  const button = saveActionButton(dialog, fileName, () => void saveDesktopZip(taskId, fileName));
  button.disabled = true;
  if (stage) stage.textContent = '请在系统窗口中选择 ZIP 的保存位置。';
  try {
    const destination = await save({
      title: '保存短剧视频 ZIP',
      defaultPath: fileName,
      filters: [{ name: 'ZIP 压缩包', extensions: ['zip'] }],
      canCreateDirectories: true,
    });
    if (!destination) {
      if (stage) stage.textContent = '尚未保存 ZIP，可再次选择保存位置。';
      return;
    }
    if (stage) stage.textContent = '正在保存 ZIP 到所选文件夹…';
    await invoke('save_video_export', {
      projectId: state.project.id,
      taskId,
      destination,
    });
    if (stage) stage.textContent = `ZIP 已保存至：${destination}`;
    button.innerHTML = `${icon('download')}<span>重新选择位置保存</span>`;
    rt().toast('视频 ZIP 已保存到所选文件夹');
  } catch (error) {
    if (stage) stage.textContent = 'ZIP 保存失败，请重新选择保存位置。';
    setError(error instanceof Error ? error.message : '保存视频 ZIP 失败');
  } finally {
    button.disabled = false;
  }
}

async function completeDownload(taskId: string, url: string, fileName: string) {
  const dialog = modal();
  if (!dialog) return;
  const stage = dialog.querySelector<HTMLElement>('[data-video-export-stage]');
  if (stage) stage.textContent = 'ZIP 已准备完成。';
  const label = dialog.querySelector<HTMLElement>('[data-video-export-progress-label]');
  if (label) label.textContent = '100%';
  const progress = dialog.querySelector<HTMLProgressElement>('[data-video-export-progress-value]');
  if (progress) progress.value = 100;
  if (isDesktopApp()) {
    saveActionButton(dialog, fileName, () => void saveDesktopZip(taskId, fileName));
    await saveDesktopZip(taskId, fileName);
    return;
  }
  const link = document.createElement('a');
  link.href = rt().resolveMediaUrl(url);
  link.download = fileName;
  link.textContent = '点击这里保存 ZIP';
  link.className = 'drama-video-export-result-link';
  dialog.querySelector<HTMLElement>('[data-video-export-progress]')?.append(link);
  link.click();
  rt().toast('视频 ZIP 已准备完成');
}

async function cancelOrClose() {
  if (!state?.taskId) { closeModal(); return; }
  const taskId = state.taskId;
  const button = modal()?.querySelector<HTMLButtonElement>('[data-video-export-cancel]');
  if (button) { button.disabled = true; button.textContent = '取消中…'; }
  try {
    const response = await fetch(`${rt().apiBaseUrl}/projects/${encodeURIComponent(state.project.id)}/video-exports/${encodeURIComponent(taskId)}/cancel`, { method: 'POST' });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    rt().toast('已取消视频打包');
    closeModal();
  } catch (error) {
    if (button) { button.disabled = false; button.textContent = '取消'; }
    setError(error instanceof Error ? error.message : '取消视频打包失败');
  }
}

function ensureExportRailItem() {
  if (document.querySelector('.game-detail')) return;
  const rail = document.querySelector<HTMLElement>('.drama-detail .drama-asset-rail');
  if (!rail || rail.querySelector('[data-drama-video-export-rail]')) return;
  if (!rail.querySelector('[data-drama-cover-rail]')) return;
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'drama-asset-rail-item drama-video-export-rail-item';
  button.dataset.dramaVideoExportRail = 'true';
  button.title = '打包下载所有剧集视频';
  button.innerHTML = `<span class="drama-asset-rail-icon">${icon('download')}</span><span>下载</span>`;
  button.addEventListener('click', () => {
    if (activeDramaProject) openModal(activeDramaProject);
    else rt().toast('请先加载短剧详情');
  });
  rail.append(button);
}

/** Configure the export dialog after the desktop API base and media protocol resolver are available. */
export function configureDramaVideoExportRuntime(value: DramaVideoExportRuntime) {
  runtime = value;
  const app = document.querySelector('#app');
  if (!app) return;
  new MutationObserver(ensureExportRailItem).observe(app, { childList: true, subtree: true });
  ensureExportRailItem();
}
