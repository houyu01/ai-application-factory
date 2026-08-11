/** Render durable video task records as the single short-drama video history. */
import type { ApiProject, DramaShot } from './models.js';
import { dramaViewState } from './drama_state.js';
import { icon } from './ui_icons.js';
import { confirmAction } from './confirmation_modal.js';
import { dramaVideoHistoryRecords, type DramaVideoHistoryRecord } from './drama_video_history.js';
import { openDramaVideoRefinementModal } from './drama_video_refinement_ui.js';

type VideoHistoryActionOptions = {
  apiBaseUrl: string;
  project: ApiProject | null;
  resolveMediaUrl: (value?: string | null) => string;
  loadDramaDetail: (id: string) => Promise<void>;
  toast: (message: string) => void;
};

type VideoHistoryRecord = DramaVideoHistoryRecord;

function selectedShot(project: ApiProject): DramaShot | undefined {
  return project.shots?.find(item => item.id === dramaViewState.shotId) || project.shots?.[0];
}

function statusLabel(record: VideoHistoryRecord): string {
  if (record.status === '生成中') return `生成中 ${Math.max(0, Number(record.progress || 0))}%`;
  if (record.status === '生成成功') return '成功';
  if (record.status === '生成失败') return '失败';
  return record.status || '未生成';
}

function isFailed(record: VideoHistoryRecord): boolean {
  return record.status === '生成失败';
}

async function selectVersionForExport(
  options: VideoHistoryActionOptions,
  shot: DramaShot,
  record: VideoHistoryRecord,
  button: HTMLButtonElement,
) {
  button.disabled = true;
  try {
    const response = await fetch(
      `${options.apiBaseUrl}/projects/${encodeURIComponent(options.project!.id)}/shots/${encodeURIComponent(shot.id)}/versions/${encodeURIComponent(record.id)}/export-selection`,
      { method: 'PUT' },
    );
    if (!response.ok) {
      const payload = await response.json().catch(() => ({})) as { detail?: string };
      throw new Error(payload.detail || `HTTP ${response.status}`);
    }
    options.toast('已设为使用版本，打包下载时会默认选择它');
    await options.loadDramaDetail(options.project!.id);
  } catch (error) {
    button.disabled = false;
    options.toast(error instanceof Error ? error.message : '设置使用版本失败');
  }
}

/** Match a history record to the video currently rendered in the preview panel. */
function isSelectedRecord(options: VideoHistoryActionOptions, record: VideoHistoryRecord): boolean {
  if (!record.url) return false;
  if (dramaViewState.videoUrl !== null) return dramaViewState.videoUrl === record.url;
  return document.querySelector<HTMLVideoElement>('#drama-video-player')?.getAttribute('src') === options.resolveMediaUrl(record.url);
}


async function deleteHistoryRecord(
  options: VideoHistoryActionOptions,
  shot: DramaShot,
  record: VideoHistoryRecord,
  button: HTMLButtonElement,
) {
  if (!await confirmAction({ title: '删除视频历史？', description: '对应视频文件和生成记录会一并删除，且无法恢复。', confirmLabel: '删除视频' })) return;
  button.disabled = true;
  try {
    const response = await fetch(
      `${options.apiBaseUrl}/projects/${encodeURIComponent(options.project!.id)}/shots/${encodeURIComponent(shot.id)}/videos/${encodeURIComponent(record.id)}`,
      { method: 'DELETE' },
    );
    if (!response.ok) {
      const payload = await response.json().catch(() => ({})) as { detail?: string };
      throw new Error(payload.detail || `HTTP ${response.status}`);
    }
    if (dramaViewState.videoUrl === record.url) dramaViewState.videoUrl = null;
    options.toast('视频历史已删除');
    await options.loadDramaDetail(options.project!.id);
  } catch (error) {
    button.disabled = false;
    options.toast(error instanceof Error ? error.message : '删除视频历史失败');
  }
}

function createStatus(record: VideoHistoryRecord): HTMLElement {
  const status = document.createElement('span');
  status.className = `status ${record.status === '生成中' ? 'running' : record.status === '生成成功' ? 'success' : record.status === '生成失败' ? 'failed' : ''}`;
  status.textContent = statusLabel(record);
  return status;
}

function attachHistoryErrorTooltip(button: HTMLButtonElement, message: string) {
  let tooltip: HTMLElement | null = null;
  let hideTimer: number | undefined;
  const cancelHide = () => {
    if (hideTimer !== undefined) window.clearTimeout(hideTimer);
    hideTimer = undefined;
  };
  const hide = () => {
    cancelHide();
    tooltip?.remove();
    tooltip = null;
  };
  const scheduleHide = () => {
    cancelHide();
    // The tooltip sits above its icon, leaving a tiny travel gap. Delaying
    // removal lets the pointer enter it and select error text for copying.
    hideTimer = window.setTimeout(hide, 180);
  };
  const show = () => {
    cancelHide();
    if (tooltip) return;
    tooltip = document.createElement('div');
    tooltip.className = 'drama-history-error-tooltip';
    tooltip.role = 'tooltip';
    tooltip.textContent = message;
    tooltip.addEventListener('mouseenter', cancelHide);
    tooltip.addEventListener('mouseleave', scheduleHide);
    document.body.append(tooltip);
    const anchor = button.getBoundingClientRect();
    const bounds = tooltip.getBoundingClientRect();
    const left = Math.min(window.innerWidth - bounds.width - 8, Math.max(8, anchor.left + anchor.width / 2 - bounds.width / 2));
    tooltip.style.left = `${left}px`;
    tooltip.style.top = `${Math.max(8, anchor.top - bounds.height - 8)}px`;
  };
  button.addEventListener('mouseenter', show);
  button.addEventListener('mouseleave', scheduleHide);
  button.addEventListener('focus', show);
  button.addEventListener('blur', scheduleHide);
}

function createHistoryEntry(
  options: VideoHistoryActionOptions,
  shot: DramaShot,
  record: VideoHistoryRecord,
  displayVersionNo: number,
): HTMLElement {
  const entry = document.createElement('article');
  entry.className = `drama-history-entry ${isFailed(record) ? 'is-failed' : ''}`;
  entry.classList.toggle('is-selected', isSelectedRecord(options, record));
  const preview = document.createElement(record.url && !isFailed(record) ? 'button' : 'div');
  preview.className = 'drama-history-preview';
  if (preview instanceof HTMLButtonElement) {
    preview.type = 'button';
    preview.title = '预览视频';
    preview.addEventListener('click', () => {
      dramaViewState.videoUrl = record.url || null;
      document.querySelectorAll('.drama-history-entry.is-selected').forEach(item => item.classList.remove('is-selected'));
      entry.classList.add('is-selected');
      void options.loadDramaDetail(options.project!.id);
    });
    const video = document.createElement('video');
    video.src = options.resolveMediaUrl(record.url);
    video.muted = true;
    video.playsInline = true;
    video.preload = 'metadata';
    preview.append(video);
  } else if (isFailed(record)) {
    preview.innerHTML = '<span class="drama-history-failed-icon" aria-hidden="true">!</span>';
  } else if (record.status === '生成中') {
    preview.innerHTML = '<span class="generation-spinner" aria-hidden="true"></span>';
  } else {
    preview.innerHTML = icon('video');
  }
  const details = document.createElement('div');
  details.className = 'drama-history-details';
  const title = document.createElement('strong');
  title.textContent = `v${record.versionNo ?? displayVersionNo}`;
  details.append(title, createStatus(record));
  const actions = document.createElement('div');
  actions.className = 'drama-history-actions';
  if (isFailed(record)) {
    const errorMessage = record.error?.trim() || '视频生成失败，请稍后重试。';
    const tip = document.createElement('button');
    tip.type = 'button';
    tip.className = 'drama-history-error-tip';
    tip.setAttribute('aria-label', `查看失败原因：${errorMessage}`);
    tip.innerHTML = icon('info');
    attachHistoryErrorTooltip(tip, errorMessage);
    actions.append(tip);
  }
  let download: HTMLAnchorElement | null = null;
  if (record.url) {
    download = document.createElement('a');
    download.className = 'drama-history-download';
    download.href = options.resolveMediaUrl(record.url);
    download.download = 'drama-video.mp4';
    download.target = '_blank';
    download.rel = 'noopener';
    download.title = '下载视频';
    download.setAttribute('aria-label', '下载视频');
    download.innerHTML = icon('download');
  }
  let refine: HTMLButtonElement | null = null;
  let useVersion: HTMLButtonElement | null = null;
  if (record.url && record.status === '生成成功') {
    useVersion = document.createElement('button');
    useVersion.type = 'button';
    useVersion.className = 'drama-history-use-version';
    useVersion.disabled = Boolean(record.selectedForExport);
    useVersion.title = record.selectedForExport ? '当前使用版本' : '设为使用版本';
    useVersion.setAttribute('aria-label', useVersion.title);
    useVersion.textContent = '✓';
    useVersion.addEventListener('click', () => void selectVersionForExport(options, shot, record, useVersion!));
    refine = document.createElement('button');
    refine.type = 'button';
    refine.className = 'drama-history-refine';
    refine.title = '微调视频';
    refine.setAttribute('aria-label', '微调视频');
    refine.innerHTML = icon('wrench');
    refine.addEventListener('click', () => openDramaVideoRefinementModal({
      apiBaseUrl: options.apiBaseUrl,
      project: options.project!,
      record,
      resolveMediaUrl: options.resolveMediaUrl,
      reloadProject: options.loadDramaDetail,
      shot,
      toast: options.toast,
    }));
  }
  const remove = document.createElement('button');
  remove.type = 'button';
  remove.className = 'drama-history-delete';
  remove.disabled = !record.id;
  remove.title = record.id ? '删除视频历史' : '该记录无法删除';
  remove.setAttribute('aria-label', remove.title);
  remove.innerHTML = icon('trash');
  if (record.id) remove.addEventListener('click', () => void deleteHistoryRecord(options, shot, record, remove));
  if (useVersion && refine && download) {
    const successActions = document.createElement('div');
    successActions.className = 'drama-history-success-actions';
    successActions.append(useVersion, refine, remove, download);
    actions.append(successActions);
  } else {
    if (download) actions.append(download);
    actions.append(remove);
  }
  entry.append(preview, details, actions);
  return entry;
}

function historySignature(shot: DramaShot, records: VideoHistoryRecord[]): string {
  return JSON.stringify({
    shotId: shot.id,
    records: records.map(record => [record.id, record.status, record.url, record.error, record.progress, record.refinementPrompt, record.selectedForExport, record.createdAt]),
  });
}

/** Replace the legacy preview cards with durable success, failure, and running records. */
export function syncDramaVideoHistoryActions(options: VideoHistoryActionOptions) {
  const history = document.querySelector<HTMLElement>('.drama-video-history');
  const project = options.project;
  const shot = project && selectedShot(project);
  if (!history || !project || !shot) return;
  const records = dramaVideoHistoryRecords(shot);
  const signature = historySignature(shot, records);
  if (history.dataset.dramaHistorySignature === signature) return;
  history.dataset.dramaHistorySignature = signature;
  const heading = document.createElement('div');
  heading.className = 'section-title';
  const headingText = document.createElement('div');
  const title = document.createElement('h3');
  title.textContent = '视频历史';
  headingText.append(title);
  const count = document.createElement('span');
  count.textContent = `${records.length} 条记录`;
  heading.append(headingText, count);
  const scroll = document.createElement('div');
  scroll.className = 'drama-history-scroll';
  if (records.length) records.forEach((record, index) => scroll.append(createHistoryEntry(options, shot, record, index + 1)));
  else {
    const empty = document.createElement('p');
    empty.className = 'muted drama-history-empty';
    empty.textContent = '暂无视频历史';
    scroll.append(empty);
  }
  history.replaceChildren(heading, scroll);
}
