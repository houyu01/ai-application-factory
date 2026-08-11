/** Durable serial and parallel controls for generating every interactive-game node video. */

import './game_video_batch_generation.css';
import type { Game, GameNode, GameNodeVideoHistory, GameTask } from './models.js';
import { captureDramaVideoFrame } from './drama_video_frame_capture.js';
import { serialGameBatchProgress } from './game_video_batch_state.js';

const SERIAL_VIDEO_BATCH = 'serial_game_node_video_batch';
const FRAME_RETRY_DELAY_MS = 15_000;

type SerialSnapshot = {
  completed_count?: number;
  current_node_id?: string | null;
  current_task_id?: string | null;
  next_index?: number;
  total_count?: number;
};

type Options = {
  apiBaseUrl: string;
  game: Game;
  reloadGame: (gameId: string) => Promise<void>;
  resolveMediaUrl: (value?: string | null) => string;
  setGenerationButtonLoading: (button: HTMLButtonElement, loading: boolean, idleText: string) => void;
  toast: (message: string) => void;
};

let current: Options | null = null;
let openMenu: HTMLElement | null = null;
let openMenuHost: HTMLElement | null = null;
let boundDocumentEvents = false;
let lastAdvanceKey = '';
let retryAfter = 0;

function serialSnapshot(task: GameTask): SerialSnapshot {
  return (task.input_snapshot || {}) as SerialSnapshot;
}

function activeSerialBatch(game: Game) {
  return (game.tasks || []).find(task => task.type === SERIAL_VIDEO_BATCH && task.status === '生成中');
}

function nodeVideoRunning(game: Game) {
  return (game.tasks || []).some(task => task.type === 'node_video_generation' && task.status === '生成中');
}

function number(value: unknown) {
  return Number.isFinite(Number(value)) ? Number(value) : 0;
}

async function readError(response: Response) {
  const payload = await response.json().catch(() => ({})) as { detail?: unknown };
  return typeof payload.detail === 'string' ? payload.detail : `HTTP ${response.status}`;
}

function closeMenu() {
  if (!openMenu) return;
  openMenu.setAttribute('hidden', '');
  openMenu.style.removeProperty('position');
  openMenu.style.removeProperty('top');
  openMenu.style.removeProperty('right');
  openMenuHost?.append(openMenu);
  openMenuHost?.querySelector<HTMLButtonElement>('[data-game-video-batch-toggle]')?.setAttribute('aria-expanded', 'false');
  openMenu = null;
  openMenuHost = null;
}

/** Render the overflow menu in the viewport so the scrolling toolbar cannot clip it. */
function positionMenu(menu: HTMLElement, toggle: HTMLButtonElement) {
  document.body.append(menu);
  const toggleRect = toggle.getBoundingClientRect();
  const menuHeight = menu.getBoundingClientRect().height;
  const top = toggleRect.bottom + 6 + menuHeight <= window.innerHeight
    ? toggleRect.bottom + 6
    : Math.max(8, toggleRect.top - 6 - menuHeight);
  menu.style.position = 'fixed';
  menu.style.top = `${top}px`;
  menu.style.right = `${Math.max(8, window.innerWidth - toggleRect.right)}px`;
}

function currentVersion(game: Game, snapshot: SerialSnapshot): GameNodeVideoHistory | undefined {
  const node = game.nodes?.find(item => item.id === snapshot.current_node_id);
  return node?.video_history?.find(item => item.id === snapshot.current_task_id || item.task_id === snapshot.current_task_id);
}

async function advanceBatch(options: Options, batch: GameTask, lastFrameDataUrl?: string) {
  const response = await fetch(`${options.apiBaseUrl}/games/${encodeURIComponent(options.game.id)}/videos/serial/${encodeURIComponent(batch.id)}/advance`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(lastFrameDataUrl ? { last_frame_data_url: lastFrameDataUrl } : {}),
  });
  if (!response.ok) throw new Error(await readError(response));
  await options.reloadGame(options.game.id);
}

async function resumeSerialBatch(options: Options) {
  const batch = activeSerialBatch(options.game);
  if (!batch) { lastAdvanceKey = ''; return; }
  const snapshot = serialSnapshot(batch);
  const taskId = snapshot.current_task_id || '';
  const version = taskId ? currentVersion(options.game, snapshot) : undefined;
  const key = `${batch.id}:${taskId}:${version?.status || 'start'}`;
  if (key === lastAdvanceKey || Date.now() < retryAfter || (taskId && !version) || version?.status === '生成中') return;
  lastAdvanceKey = key;
  try {
    if (version?.status === '生成成功') {
      const last = number(snapshot.next_index) >= number(snapshot.total_count);
      const frame = last ? undefined : await captureDramaVideoFrame(version.url || '', 'last', options.resolveMediaUrl);
      if (!last && !frame) {
        retryAfter = Date.now() + FRAME_RETRY_DELAY_MS;
        lastAdvanceKey = '';
        options.toast('无法提取上一节点的尾帧，串行生成将在稍后自动重试');
        return;
      }
      await advanceBatch(options, batch, frame || undefined);
    } else if (!version || version.status === '生成失败' || version.status === '已取消') {
      await advanceBatch(options, batch);
    }
  } catch (error) {
    lastAdvanceKey = '';
    retryAfter = Date.now() + FRAME_RETRY_DELAY_MS;
    options.toast(`串行生成继续失败：${error instanceof Error ? error.message : '请稍后重试'}`);
  }
}

async function startSerialBatch(options: Options) {
  const response = await fetch(`${options.apiBaseUrl}/games/${encodeURIComponent(options.game.id)}/videos/serial`, { method: 'POST' });
  if (!response.ok) throw new Error(await readError(response));
  options.toast('已开始串行生成：下一视频节点将自动使用上一节点的尾帧作为首帧');
  await options.reloadGame(options.game.id);
}

async function queueNodeVideo(options: Options, node: GameNode) {
  const response = await fetch(`${options.apiBaseUrl}/games/${encodeURIComponent(options.game.id)}/nodes/${encodeURIComponent(node.id)}/video`, { method: 'POST' });
  if (!response.ok) throw new Error(`${node.title}：${await readError(response)}`);
  return response.json() as Promise<GameTask>;
}

async function startParallelBatch(options: Options, button: HTMLButtonElement) {
  const nodes = options.game.nodes || [];
  if (!nodes.length) { options.toast('当前游戏没有可生成视频的节点'); return; }
  options.setGenerationButtonLoading(button, true, '▣ 生成所有视频');
  try {
    const queued = await Promise.allSettled(nodes.map(node => queueNodeVideo(options, node)));
    const failed = queued.filter(item => item.status === 'rejected');
    if (failed.length === 0) options.toast('已创建全部节点视频任务');
    else options.toast(`已创建 ${nodes.length - failed.length} 个节点视频任务；${failed.length} 个未创建`);
    await options.reloadGame(options.game.id);
    failed.forEach(item => console.error('节点视频任务创建失败', item.reason));
  } catch (error) {
    options.setGenerationButtonLoading(button, false, '▣ 生成所有视频');
    options.toast(`全部节点视频任务创建失败：${error instanceof Error ? error.message : '请稍后重试'}`);
  }
}

function lockControlsForActiveSerialBatch(game: Game) {
  const batch = activeSerialBatch(game);
  const primary = document.querySelector<HTMLButtonElement>('#game-generate-all-videos');
  if (primary && current) current.setGenerationButtonLoading(primary, nodeVideoRunning(game), '▣ 生成所有视频');
  if (!batch) return;
  document.querySelectorAll<HTMLButtonElement>('[data-game-video-batch-actions] button').forEach(button => { button.disabled = true; });
}

function bindMenu(wrapper: HTMLElement) {
  const toggle = wrapper.querySelector<HTMLButtonElement>('[data-game-video-batch-toggle]');
  const menu = wrapper.querySelector<HTMLElement>('[data-game-video-batch-menu]');
  const serial = wrapper.querySelector<HTMLButtonElement>('[data-game-generate-videos-serial]');
  const parallel = wrapper.querySelector<HTMLButtonElement>('[data-game-generate-videos-parallel]');
  const primary = wrapper.querySelector<HTMLButtonElement>('#game-generate-all-videos');
  if (!toggle || !menu || !serial || !parallel || !primary) return;
  toggle.addEventListener('click', event => {
    event.stopPropagation();
    const opening = menu.hasAttribute('hidden');
    closeMenu();
    if (opening) {
      menu.removeAttribute('hidden');
      positionMenu(menu, toggle);
      toggle.setAttribute('aria-expanded', 'true');
      openMenu = menu;
      openMenuHost = wrapper;
    }
  });
  serial.addEventListener('click', () => {
    const options = current;
    if (!options) return;
    closeMenu(); serial.disabled = true;
    void startSerialBatch(options).catch(error => options.toast(`串行生成启动失败：${error instanceof Error ? error.message : '请稍后重试'}`)).finally(() => { serial.disabled = false; });
  });
  parallel.addEventListener('click', () => { closeMenu(); primary.click(); });
  primary.addEventListener('click', () => { if (current) void startParallelBatch(current, primary); });
}

function bindDocumentEvents() {
  if (boundDocumentEvents) return;
  document.addEventListener('click', closeMenu);
  document.addEventListener('keydown', event => { if (event.key === 'Escape') closeMenu(); });
  boundDocumentEvents = true;
}

/** Bind the game toolbar split button and continue an unfinished serial batch after refresh or restart. */
export function syncGameVideoBatchGeneration(options: Options) {
  closeMenu();
  current = options;
  const wrapper = document.querySelector<HTMLElement>('[data-game-video-batch-actions]');
  if (wrapper && wrapper.dataset.bound !== 'true') { wrapper.dataset.bound = 'true'; bindMenu(wrapper); }
  lockControlsForActiveSerialBatch(options.game);
  bindDocumentEvents();
  void resumeSerialBatch(options);
}

/** Feed polling snapshots to the serial coordinator so it can enqueue the next node immediately. */
export function refreshGameVideoBatchGeneration(game: Game) {
  if (!current || current.game.id !== game.id) return;
  current = { ...current, game };
  lockControlsForActiveSerialBatch(game);
  void resumeSerialBatch(current);
}

export { serialGameBatchProgress };
