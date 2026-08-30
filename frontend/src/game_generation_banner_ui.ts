/** Live task banner for screenplay expansion and graph decomposition in the game workbench. */

import type { Game, GameTask } from './models.js';
import {
  generationElapsedTitleMarkup,
  parseGenerationTimestampMs,
  syncGenerationElapsedTitle,
} from './generation_elapsed_ui.ts';

const GAME_GENERATION_STEPS = ['等待执行', '扩写剧本', '拆分视频节点', '保存图谱'];

type GenerationCopy = {
  task: GameTask;
  failed: boolean;
  graphPlanning: boolean;
  step: number;
  progress: number;
  title: string;
  detail: string;
  preview: string;
  receivedLabel: string;
  nodeCount: number;
  edgeCount: number;
  timerKey: string;
  timerStartedAt?: string | null;
};

function generationTask(game: Game) {
  const tasks = [...(game.tasks || [])].reverse();
  const supported = (task: GameTask) => ['game_script_expansion', 'game_graph_decomposition'].includes(task.type);
  return tasks.find(task => task.status === '生成中' && supported(task))
    || tasks.find(task => task.status === '生成失败' && supported(task));
}

function taskProgress(task: GameTask) {
  const value = Number(task.progress || 0);
  return Number.isFinite(value) ? Math.max(0, Math.min(100, Math.round(value))) : 0;
}

function currentStep(task: GameTask, graphPlanning: boolean, progress: number) {
  if (!graphPlanning) return progress >= 5 ? 1 : 0;
  return progress >= 92 || /图谱骨架已保存|正在写入视频节点|正在复核选择边|校验未通过|仅重新生成选择边/.test(task.stage || '') ? 3 : 2;
}

/** Keep script expansion and its automatically queued graph decomposition on one stopwatch. */
function generationTimerKey(game: Game, task: GameTask) {
  const tasks = game.tasks || [];
  const taskIndex = tasks.findIndex(item => item.id === task.id);
  if (task.type === 'game_graph_decomposition' && taskIndex >= 0) {
    for (let index = taskIndex - 1; index >= 0; index -= 1) {
      if (tasks[index].type === 'game_script_expansion') return `game:${game.id}:${tasks[index].id}`;
    }
  }
  return `game:${game.id}:${task.id}`;
}

function generationTimerStartedAt(game: Game, task: GameTask) {
  const tasks = game.tasks || [];
  const taskIndex = tasks.findIndex(item => item.id === task.id);
  if (task.type === 'game_graph_decomposition' && taskIndex >= 0) {
    for (let index = taskIndex - 1; index >= 0; index -= 1) {
      const previous = tasks[index];
      if (previous.type === 'game_script_expansion') {
        const graphStart = task.started_at || task.created_at;
        return followsExpansionRun(previous, graphStart)
          ? previous.started_at || previous.created_at
          : graphStart;
      }
    }
  }
  return task.started_at || task.created_at;
}

/** Share one stopwatch across expansion → graph, but restart after an independent graph retry. */
function followsExpansionRun(expansion: GameTask, graphStart?: string | null) {
  if (!expansion.completed_at) return true;
  const graphMs = parseGenerationTimestampMs(graphStart);
  const expansionEndMs = parseGenerationTimestampMs(expansion.completed_at);
  if (!Number.isFinite(graphMs) || !Number.isFinite(expansionEndMs)) return true;
  return Math.abs(graphMs - expansionEndMs) <= 10 * 60_000;
}

/** List cards and the canvas badge follow live bootstrap tasks, not a stale aggregate row. */
export function gameFlowStatus(game: Game) {
  const task = generationTask(game);
  return task?.status === '生成中' ? '生成中' : game.status;
}

/** Derive creator-visible live output from the durable task snapshot and saved screenplay. */
export function gameGenerationCopy(game: Game): GenerationCopy | undefined {
  const task = generationTask(game);
  if (!task) return undefined;
  const failed = task.status === '生成失败';
  const graphPlanning = task.type === 'game_graph_decomposition';
  const snapshot = task.input_snapshot || {};
  const progress = taskProgress(task);
  const step = currentStep(task, graphPlanning, progress);
  const preview = graphPlanning
    ? String(snapshot.graph_preview || '')
    : game.expanded_script || String(snapshot.expanded_script_preview || '');
  const receivedChars = Number(snapshot.preview_received_chars) || Array.from(preview).length;
  const nodeCount = Number(snapshot.preview_node_count || 0);
  const edgeCount = Number(snapshot.preview_edge_count || 0);
  const title = failed
    ? graphPlanning ? '游戏图谱生成失败' : '互动游戏剧本扩写失败'
    : graphPlanning
      ? `第 ${step + 1}/4 步：${GAME_GENERATION_STEPS[step]}`
      : '第 2/4 步：扩写互动游戏剧本（正在实时输出）';
  const detail = failed
    ? task.error_message || `${graphPlanning ? '游戏图谱生成' : '互动游戏剧本扩写'}失败，请检查模型配置后重试。`
    : task.stage || '正在准备生成任务。';
  return {
    task,
    failed,
    graphPlanning,
    step,
    progress,
    title,
    detail,
    preview,
    receivedLabel: graphPlanning ? `骨架已接收 ${receivedChars.toLocaleString()} 字` : `剧本已接收 ${receivedChars.toLocaleString()} 字`,
    nodeCount,
    edgeCount,
    timerKey: generationTimerKey(game, task),
    timerStartedAt: generationTimerStartedAt(game, task),
  };
}

function progressMarkup(copy: GenerationCopy) {
  return `<div class="drama-decomposition-progress" data-game-generation-progress${copy.failed ? ' hidden' : ''}><ol>${GAME_GENERATION_STEPS.map((label, index) => `<li data-game-generation-step="${index}" class="${index < copy.step ? 'completed' : index === copy.step ? 'active' : ''}"${index === copy.step ? ' aria-current="step"' : ''}><i>${index + 1}</i><span>${label}</span></li>`).join('')}</ol><div class="drama-decomposition-progress-meter"><progress max="100" value="${copy.progress}"></progress><div class="drama-decomposition-progress-details"><span data-game-generation-received>${copy.receivedLabel}</span><span data-game-generation-progress-label>当前进度 ${copy.progress}%</span></div></div></div>`;
}

/** Render the initial game-generation banner before polling begins. */
export function gameGenerationBannerMarkup(game: Game, escapeHtml: (value: unknown) => string) {
  const copy = gameGenerationCopy(game);
  if (!copy) return '';
  const title = copy.failed
    ? escapeHtml(copy.title)
    : generationElapsedTitleMarkup(copy.title, copy.timerKey, copy.timerStartedAt, escapeHtml);
  const skeleton = copy.graphPlanning
    ? `<div class="game-meta" data-game-generation-skeleton><span data-game-generation-node-count>视频节点骨架：${copy.nodeCount} 个</span><span data-game-generation-edge-count>选择边：${copy.edgeCount} 条</span></div>`
    : '<div class="game-meta" data-game-generation-skeleton hidden></div>';
  return `<section class="drama-decomposition-banner${copy.failed ? ' failed' : ''}" data-game-generation-banner role="${copy.failed ? 'alert' : 'status'}"><span class="generation-spinner" aria-hidden="true"${copy.failed ? ' hidden' : ''}></span><div><span class="drama-decomposition-banner-title">${title}</span><p class="drama-decomposition-banner-detail">${escapeHtml(copy.detail)}</p>${progressMarkup(copy)}${skeleton}<pre class="drama-decomposition-banner-preview" data-game-generation-preview${copy.preview ? '' : ' hidden'} aria-live="polite" aria-atomic="false">${escapeHtml(copy.preview)}</pre><div class="drama-generation-banner-actions"><button type="button" class="ghost compact danger-button" data-game-stop-generation${copy.failed ? ' hidden' : ''}>停止生成</button></div><button type="button" class="ghost compact" id="game-retry-generation"${copy.failed ? '' : ' hidden'}>继续</button></div></section>`;
}

function setTextIfChanged(element: HTMLElement | null, value: string) {
  if (element && element.textContent !== value) element.textContent = value;
}

function syncProgress(banner: HTMLElement, copy: GenerationCopy) {
  const progress = banner.querySelector<HTMLElement>('[data-game-generation-progress]');
  if (!progress) return;
  progress.hidden = copy.failed;
  if (copy.failed) return;
  progress.querySelectorAll<HTMLElement>('[data-game-generation-step]').forEach(item => {
    const step = Number(item.dataset.gameGenerationStep);
    item.classList.toggle('completed', step < copy.step);
    item.classList.toggle('active', step === copy.step);
    item.toggleAttribute('aria-current', step === copy.step);
  });
  const meter = progress.querySelector<HTMLProgressElement>('progress');
  if (meter) meter.value = copy.progress;
  setTextIfChanged(progress.querySelector<HTMLElement>('[data-game-generation-received]'), copy.receivedLabel);
  setTextIfChanged(progress.querySelector<HTMLElement>('[data-game-generation-progress-label]'), `当前进度 ${copy.progress}%`);
}

function syncBanner(banner: HTMLElement, copy: GenerationCopy, onRetry?: (gameId: string) => void, gameId?: string, onStop?: (gameId: string, button: HTMLButtonElement) => void) {
  banner.classList.toggle('failed', copy.failed);
  banner.setAttribute('role', copy.failed ? 'alert' : 'status');
  banner.querySelector<HTMLElement>('.generation-spinner')?.toggleAttribute('hidden', copy.failed);
  const title = banner.querySelector<HTMLElement>('.drama-decomposition-banner-title');
  if (title) {
    if (copy.failed) setTextIfChanged(title, copy.title);
    else syncGenerationElapsedTitle(title, copy.title, copy.timerKey, copy.timerStartedAt);
  }
  setTextIfChanged(banner.querySelector<HTMLElement>('.drama-decomposition-banner-detail'), copy.detail);
  syncProgress(banner, copy);
  const skeleton = banner.querySelector<HTMLElement>('[data-game-generation-skeleton]');
  if (skeleton) {
    skeleton.hidden = !copy.graphPlanning;
    if (copy.graphPlanning && !skeleton.querySelector('[data-game-generation-node-count]')) {
      skeleton.innerHTML = '<span data-game-generation-node-count></span><span data-game-generation-edge-count></span>';
    }
    setTextIfChanged(skeleton.querySelector<HTMLElement>('[data-game-generation-node-count]'), `视频节点骨架：${copy.nodeCount} 个`);
    setTextIfChanged(skeleton.querySelector<HTMLElement>('[data-game-generation-edge-count]'), `选择边：${copy.edgeCount} 条`);
  }
  const preview = banner.querySelector<HTMLElement>('[data-game-generation-preview]');
  if (preview) {
    const followsLatest = preview.scrollTop + preview.clientHeight >= preview.scrollHeight - 24;
    setTextIfChanged(preview, copy.preview);
    preview.hidden = !copy.preview;
    if (followsLatest) preview.scrollTop = preview.scrollHeight;
  }
  const retry = banner.querySelector<HTMLButtonElement>('#game-retry-generation');
  if (retry) {
    retry.hidden = !copy.failed;
    retry.onclick = copy.failed && onRetry && gameId ? () => onRetry(gameId) : null;
  }
  const stop = banner.querySelector<HTMLButtonElement>('[data-game-stop-generation]');
  if (stop) { stop.hidden = copy.failed; stop.onclick = !copy.failed && onStop && gameId ? () => onStop(gameId, stop) : null; }
}

/** Patch the workbench banner after a background task refresh without recreating the editor. */
export function syncGameGenerationBanner(
  game: Game,
  escapeHtml: (value: unknown) => string,
  onRetry?: (gameId: string) => void,
  onStop?: (gameId: string, button: HTMLButtonElement) => void,
) {
  const detail = document.querySelector<HTMLElement>('.game-detail');
  if (!detail) return;
  const copy = gameGenerationCopy(game);
  const current = detail.querySelector<HTMLElement>('[data-game-generation-banner]');
  if (!copy) {
    current?.remove();
    return;
  }
  if (current) {
    syncBanner(current, copy, onRetry, game.id, onStop);
    return;
  }
  const toolbar = detail.querySelector<HTMLElement>('.drama-detail-toolbar');
  if (!toolbar) return;
  const wrapper = document.createElement('div');
  wrapper.innerHTML = gameGenerationBannerMarkup(game, escapeHtml);
  const banner = wrapper.firstElementChild as HTMLElement | null;
  if (!banner) return;
  toolbar.insertAdjacentElement('afterend', banner);
  syncBanner(banner, copy, onRetry, game.id, onStop);
}
