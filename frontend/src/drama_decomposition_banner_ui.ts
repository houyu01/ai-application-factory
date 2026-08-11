/**
 * Project-level status banner for every screenplay-owning workflow.
 *
 * The detail toolbar observer calls this after a newly-created short drama is
 * rendered. It gives creators persistent feedback while the durable
 * `script_decomposition` or `script_expansion` task is extracting shots and reusable assets. The
 * banner is removed as soon as that task no longer runs, including after a
 * recovered task finishes following a local worker restart.
 */
import type { ApiProject, GenerationTask } from './models.js';

const DECOMPOSITION_STEPS = ['等待执行', '扩写剧本', '拆解分镜', '保存编辑内容'];
const EXPANDING_SCREENPLAY_TITLE = '扩写剧本(点击上方“剧本”查看实时剧本)';

function screenplayTask(project: ApiProject | null) {
  const tasks = [...(project?.tasks || [])].reverse();
  const active = tasks.find(task => task.status === '生成中'
    && (task.type === 'script_decomposition' || task.type === 'script_expansion'));
  if (active) return active;
  const bootstrap = tasks.find(task => task.type === 'script_decomposition');
  if (bootstrap?.status === '生成失败') return bootstrap;
  return tasks.find(task => task.type === 'script_decomposition' || task.type === 'script_expansion');
}

/** Avoid creating DOM mutations when an observer rechecks unchanged task data. */
function setTextIfChanged(element: HTMLElement | null, value: string) {
  if (element && element.textContent !== value) element.textContent = value;
}

function taskProgress(task: GenerationTask): number {
  const value = Number(task.progress || 0);
  return Number.isFinite(value) ? Math.max(0, Math.min(100, Math.round(value))) : 0;
}

function currentStep(task: GenerationTask): number {
  const progress = taskProgress(task);
  const stage = task.stage || '';
  if (progress >= 90 || /保存|写入|持久/.test(stage)) return 3;
  if (progress >= 65 || /拆解|分镜|素材/.test(stage)) return 2;
  if (progress >= 5 || /扩写|故事圣经|联网/.test(stage)) return 1;
  return 0;
}

function expandedScreenplay(project: ApiProject): string {
  return project.expanded_script || '';
}

/** Read the durable cumulative stream count that the storyboard worker writes into its stage. */
function storyboardReceivedChars(task: GenerationTask): number | undefined {
  if (task.type !== 'script_decomposition') return undefined;
  const value = task.stage?.match(/累计已接收\s*([\d,]+)\s*字/)?.[1];
  if (!value) return undefined;
  const chars = Number(value.replaceAll(',', ''));
  return Number.isFinite(chars) && chars >= 0 ? chars : undefined;
}

export function generationCopy(project: ApiProject, task: GenerationTask) {
  if (task.type === 'script_expansion') {
    return {
      step: 1,
      progress: taskProgress(task),
      receivedChars: undefined,
      title: EXPANDING_SCREENPLAY_TITLE,
      detail: task.error_message?.trim() || task.stage || '正在基于已保存的剧本继续扩写。',
    };
  }
  const step = currentStep(task);
  const waitingForWorker = project.queue_state === 'queued' && !task.stage && taskProgress(task) === 0;
  const queuePosition = project.queue_position || 1;
  return {
    step,
    progress: taskProgress(task),
    receivedChars: storyboardReceivedChars(task),
    title: step === 1
      ? EXPANDING_SCREENPLAY_TITLE
      : `第 ${step + 1}/4 步：${DECOMPOSITION_STEPS[step]}`,
    detail: task.error_message?.trim()
      || task.stage
      || (waitingForWorker
        ? `任务正在语言模型队列中，当前排在第 ${queuePosition} 位。`
        : '正在准备剧本和已保存的生成进度。'),
  };
}

function progressMarkup() {
  return `<div class="drama-decomposition-progress" data-drama-decomposition-progress><ol>${DECOMPOSITION_STEPS.map((label, index) => `<li data-drama-decomposition-step="${index}"><i>${index + 1}</i><span>${label}</span></li>`).join('')}</ol><div class="drama-decomposition-progress-meter"><progress max="100" value="0"></progress><div class="drama-decomposition-progress-details"><span data-drama-decomposition-received hidden></span><span data-drama-decomposition-progress-label>当前进度 0%</span></div></div></div>`;
}

function syncProgressIndicator(banner: HTMLElement, step: number, progress: number, failed: boolean, receivedChars?: number) {
  const indicator = banner.querySelector<HTMLElement>('[data-drama-decomposition-progress]');
  if (!indicator) return;
  indicator.hidden = failed;
  if (failed) return;
  indicator.querySelectorAll<HTMLElement>('[data-drama-decomposition-step]').forEach(item => {
    const index = Number(item.dataset.dramaDecompositionStep);
    item.classList.toggle('completed', index < step);
    item.classList.toggle('active', index === step);
    if (index === step) item.setAttribute('aria-current', 'step');
    else item.removeAttribute('aria-current');
  });
  const meter = indicator.querySelector<HTMLProgressElement>('progress');
  if (meter) meter.value = progress;
  setTextIfChanged(
    indicator.querySelector<HTMLElement>('[data-drama-decomposition-progress-label]'),
    `当前进度 ${progress}%`,
  );
  const received = indicator.querySelector<HTMLElement>('[data-drama-decomposition-received]');
  if (received) {
    received.hidden = receivedChars === undefined;
    if (receivedChars !== undefined) setTextIfChanged(received, `骨架已接收 ${receivedChars.toLocaleString()} 字`);
  }
}

/** Synchronize the detail-page banner with the newest persisted screenplay task. */
export function syncDramaDecompositionBanner(project: ApiProject | null, onRetry?: (projectId: string) => void) {
  const detail = document.querySelector<HTMLElement>('.drama-detail');
  if (!detail) return;
  const current = detail.querySelector<HTMLElement>('[data-drama-decomposition-banner]');
  const task = screenplayTask(project);
  const generating = task?.status === '生成中';
  const failed = task?.status === '生成失败';
  if (!task || (!generating && !failed)) {
    current?.remove();
    return;
  }
  const copy = generationCopy(project!, task);
  const titleText = failed
    ? task.type === 'script_expansion' ? '剧本扩写失败' : '剧本生成失败'
    : copy.title;
  const detailText = failed
    ? task.error_message?.trim() || `${task.type === 'script_expansion' ? '剧本扩写' : '剧本生成'}失败，请检查语言模型配置后重试。`
    : copy.detail;
  const previewText = expandedScreenplay(project!);
  if (current) {
    const title = current.querySelector<HTMLElement>('.drama-decomposition-banner-title');
    const bannerDetail = current.querySelector<HTMLElement>('.drama-decomposition-banner-detail');
    const preview = current.querySelector<HTMLElement>('.drama-decomposition-banner-preview');
    const retry = current.querySelector<HTMLButtonElement>('[data-drama-retry-decomposition]');
    current.classList.toggle('failed', failed);
    current.setAttribute('role', failed ? 'alert' : 'status');
    current.querySelector<HTMLElement>('.generation-spinner')?.toggleAttribute('hidden', failed);
    setTextIfChanged(title, titleText);
    setTextIfChanged(bannerDetail, detailText);
    syncProgressIndicator(current, copy.step, copy.progress, failed, copy.receivedChars);
    setTextIfChanged(preview, previewText);
    if (preview) preview.hidden = !previewText;
    if (retry) {
      retry.hidden = !failed;
      if (failed && onRetry) retry.onclick = () => onRetry(project!.id);
    }
    return;
  }
  const toolbar = detail.querySelector<HTMLElement>('.drama-detail-toolbar');
  if (!toolbar) return;
  const banner = document.createElement('section');
  banner.className = `drama-decomposition-banner${failed ? ' failed' : ''}`;
  banner.dataset.dramaDecompositionBanner = 'true';
  banner.setAttribute('role', failed ? 'alert' : 'status');
  banner.innerHTML = `<span class="generation-spinner" aria-hidden="true"${failed ? ' hidden' : ''}></span><div><span class="drama-decomposition-banner-title"></span><p class="drama-decomposition-banner-detail"></p>${progressMarkup()}<pre class="drama-decomposition-banner-preview" aria-live="polite"></pre><button type="button" class="ghost compact" data-drama-retry-decomposition${failed ? '' : ' hidden'}>重试</button></div>`;
  const title = banner.querySelector<HTMLElement>('.drama-decomposition-banner-title');
  const bannerDetail = banner.querySelector<HTMLElement>('.drama-decomposition-banner-detail');
  const preview = banner.querySelector<HTMLElement>('.drama-decomposition-banner-preview');
  const retry = banner.querySelector<HTMLButtonElement>('[data-drama-retry-decomposition]');
  setTextIfChanged(title, titleText);
  setTextIfChanged(bannerDetail, detailText);
  syncProgressIndicator(banner, copy.step, copy.progress, failed, copy.receivedChars);
  setTextIfChanged(preview, previewText);
  if (preview) preview.hidden = !previewText;
  if (retry && failed && onRetry) retry.onclick = () => onRetry(project!.id);
  toolbar.insertAdjacentElement('afterend', banner);
}
