/** Durable serial/parallel controls for the project-wide storyboard video action. */
import './drama_video_batch_generation.css';
import type { ApiProject, DramaShot, DramaShotVersion, GenerationTask } from './models.js';
import { captureDramaVideoFrame } from './drama_video_frame_capture.js';

const SERIAL_VIDEO_BATCH = 'serial_shot_video_batch';
const FRAME_RETRY_DELAY_MS = 15_000;

type SerialSnapshot = {
  completed_count?: number;
  current_shot_id?: string | null;
  current_task_id?: string | null;
  next_index?: number;
  total_count?: number;
};

type BatchGenerationOptions = {
  apiBaseUrl: string;
  project: ApiProject;
  reloadProject: (projectId: string) => Promise<void>;
  resolveMediaUrl: (value?: string | null) => string;
  toast: (message: string) => void;
};

let current: BatchGenerationOptions | null = null;
let openMenu: HTMLElement | null = null;
let boundDocumentEvents = false;
let lastAdvanceKey = '';
let retryAfter = 0;

function serialSnapshot(task: GenerationTask): SerialSnapshot {
  return (task.input_snapshot || {}) as SerialSnapshot;
}

function activeSerialBatch(project: ApiProject) {
  return (project.tasks || []).find(task => task.type === SERIAL_VIDEO_BATCH && task.status === '生成中');
}

function number(value: unknown) {
  return Number.isFinite(Number(value)) ? Number(value) : 0;
}

function readError(response: Response) {
  return response.json()
    .then(payload => typeof payload?.detail === 'string' ? payload.detail : `HTTP ${response.status}`)
    .catch(() => `HTTP ${response.status}`);
}

function closeMenu() {
  openMenu?.setAttribute('hidden', '');
  openMenu?.parentElement
    ?.querySelector<HTMLButtonElement>('[data-drama-video-batch-toggle]')
    ?.setAttribute('aria-expanded', 'false');
  openMenu = null;
}

function currentVersion(project: ApiProject, snapshot: SerialSnapshot): DramaShotVersion | undefined {
  const shot = project.shots?.find(item => item.id === snapshot.current_shot_id);
  return shot?.versions?.find(item => item.task_id === snapshot.current_task_id);
}

async function advanceBatch(
  options: BatchGenerationOptions,
  batch: GenerationTask,
  lastFrameDataUrl?: string,
) {
  const response = await fetch(
    `${options.apiBaseUrl}/projects/${encodeURIComponent(options.project.id)}/videos/serial/${encodeURIComponent(batch.id)}/advance`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(lastFrameDataUrl ? { last_frame_data_url: lastFrameDataUrl } : {}),
    },
  );
  if (!response.ok) throw new Error(await readError(response));
  await options.reloadProject(options.project.id);
}

async function resumeSerialBatch(options: BatchGenerationOptions) {
  const batch = activeSerialBatch(options.project);
  if (!batch) {
    lastAdvanceKey = '';
    return;
  }
  const snapshot = serialSnapshot(batch);
  const childTaskId = snapshot.current_task_id || '';
  const version = childTaskId ? currentVersion(options.project, snapshot) : undefined;
  const key = `${batch.id}:${childTaskId}:${version?.status || 'start'}`;
  if (key === lastAdvanceKey || Date.now() < retryAfter) return;
  if (childTaskId && !version) return;
  if (version?.status === '生成中') return;
  lastAdvanceKey = key;
  try {
    if (version?.status === '生成成功') {
      const isLastShot = number(snapshot.next_index) >= number(snapshot.total_count);
      const lastFrame = isLastShot
        ? undefined
        : await captureDramaVideoFrame(version.video_url || '', 'last', options.resolveMediaUrl);
      if (!isLastShot && !lastFrame) {
        retryAfter = Date.now() + FRAME_RETRY_DELAY_MS;
        lastAdvanceKey = '';
        options.toast('无法提取上一分镜的尾帧，串行生成将在稍后自动重试');
        return;
      }
      await advanceBatch(options, batch, lastFrame || undefined);
      return;
    }
    if (!version || version.status === '生成失败' || version.status === '已取消') {
      await advanceBatch(options, batch);
    }
  } catch (error) {
    lastAdvanceKey = '';
    retryAfter = Date.now() + FRAME_RETRY_DELAY_MS;
    options.toast(`串行生成继续失败：${error instanceof Error ? error.message : '请稍后重试'}`);
  }
}

async function startSerialBatch(options: BatchGenerationOptions) {
  const response = await fetch(
    `${options.apiBaseUrl}/projects/${encodeURIComponent(options.project.id)}/videos/serial`,
    { method: 'POST' },
  );
  if (!response.ok) throw new Error(await readError(response));
  options.toast('已开始串行生成：下一分镜将自动使用上一分镜的尾帧作为首帧');
  await options.reloadProject(options.project.id);
}

function bindMenu(wrapper: HTMLElement) {
  const toggle = wrapper.querySelector<HTMLButtonElement>('[data-drama-video-batch-toggle]');
  const menu = wrapper.querySelector<HTMLElement>('[data-drama-video-batch-menu]');
  const serial = wrapper.querySelector<HTMLButtonElement>('[data-drama-generate-videos-serial]');
  const parallel = wrapper.querySelector<HTMLButtonElement>('[data-drama-generate-videos-parallel]');
  const primary = wrapper.querySelector<HTMLButtonElement>('#drama-generate-all-videos');
  if (!toggle || !menu || !serial || !parallel || !primary) return;
  toggle.addEventListener('click', event => {
    event.stopPropagation();
    const opening = menu.hasAttribute('hidden');
    closeMenu();
    if (opening) {
      menu.removeAttribute('hidden');
      toggle.setAttribute('aria-expanded', 'true');
      openMenu = menu;
    }
  });
  serial.addEventListener('click', () => {
    const options = current;
    if (!options) return;
    closeMenu();
    serial.disabled = true;
    void startSerialBatch(options)
      .catch(error => options.toast(`串行生成启动失败：${error instanceof Error ? error.message : '请稍后重试'}`))
      .finally(() => { serial.disabled = false; });
  });
  parallel.addEventListener('click', () => {
    closeMenu();
    primary.click();
  });
}

function bindDocumentEvents() {
  if (boundDocumentEvents) return;
  document.addEventListener('click', closeMenu);
  document.addEventListener('keydown', event => {
    if (event.key === 'Escape') closeMenu();
  });
  boundDocumentEvents = true;
}

function lockControlsForActiveSerialBatch(project: ApiProject) {
  if (!activeSerialBatch(project)) return;
  document.querySelectorAll<HTMLButtonElement>('[data-drama-video-batch-actions] button').forEach(button => {
    button.disabled = true;
  });
}

/** Bind the toolbar split button and resume any persisted serial batch after a detail refresh or application restart. */
export function syncDramaVideoBatchGeneration(options: BatchGenerationOptions) {
  current = options;
  const wrapper = document.querySelector<HTMLElement>('[data-drama-video-batch-actions]');
  if (wrapper && wrapper.dataset.bound !== 'true') {
    wrapper.dataset.bound = 'true';
    bindMenu(wrapper);
  }
  lockControlsForActiveSerialBatch(options.project);
  bindDocumentEvents();
  void resumeSerialBatch(options);
}

/** Receive task-poll updates so a completed video immediately unlocks the next persisted serial task. */
export function refreshDramaVideoBatchGeneration(project: ApiProject) {
  if (!current || current.project.id !== project.id) return;
  current = { ...current, project };
  lockControlsForActiveSerialBatch(project);
  void resumeSerialBatch(current);
}

export function serialBatchProgress(project: ApiProject): { completed: number; total: number } | null {
  const batch = activeSerialBatch(project);
  if (!batch) return null;
  const snapshot = serialSnapshot(batch);
  return { completed: number(snapshot.completed_count), total: number(snapshot.total_count) };
}
