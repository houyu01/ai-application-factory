/** Keep an interactive-game node video's cancellation controls in sync with its durable task. */

import type { Game, GameNode, GameTask } from './models.js';
import './game_node_video_cancellation.css';

type Options = {
  apiBaseUrl: string;
  game: Game;
  node: GameNode;
  task?: GameTask;
  toast: (message: string) => void;
  onCancelled: () => Promise<void> | void;
};

const states = new WeakMap<HTMLElement, Options>();
let openMenu: HTMLElement | null = null;
let openTaskKey: string | null = null;
let escapeListenerReady = false;

const taskKey = ({ game, node, task }: Options) => task?.id ? `${game.id}:${node.id}:${task.id}` : null;

function closeMenu() {
  openMenu?.parentElement
    ?.querySelector<HTMLButtonElement>('[data-game-node-video-cancel-toggle]')
    ?.setAttribute('aria-expanded', 'false');
  openMenu?.setAttribute('hidden', '');
  openMenu = null;
  openTaskKey = null;
}

function readError(response: Response) {
  return response.json()
    .then(body => typeof body?.detail === 'string' ? body.detail : `HTTP ${response.status}`)
    .catch(() => `HTTP ${response.status}`);
}

function setCancelling(wrapper: HTMLElement, cancelling: boolean) {
  const action = wrapper.querySelector<HTMLButtonElement>('[data-game-node-cancel-video]');
  const trigger = wrapper.querySelector<HTMLButtonElement>('[data-game-node-video-cancel-toggle]');
  if (action) {
    action.disabled = cancelling;
    action.textContent = cancelling ? '取消中…' : '取消生成';
  }
  if (trigger) trigger.disabled = cancelling;
}

function bindControls(wrapper: HTMLElement) {
  const trigger = wrapper.querySelector<HTMLButtonElement>('[data-game-node-video-cancel-toggle]');
  const menu = wrapper.querySelector<HTMLElement>('[data-game-node-video-cancel-menu]');
  const cancel = wrapper.querySelector<HTMLButtonElement>('[data-game-node-cancel-video]');
  if (!trigger || !menu || !cancel) return;
  trigger.addEventListener('pointerdown', event => event.stopPropagation());
  menu.addEventListener('pointerdown', event => event.stopPropagation());
  menu.addEventListener('click', event => event.stopPropagation());
  trigger.addEventListener('click', event => {
    event.stopPropagation();
    const opening = menu.hasAttribute('hidden');
    closeMenu();
    const options = states.get(wrapper);
    if (!opening || options?.task?.status !== '生成中') return;
    menu.removeAttribute('hidden');
    openMenu = menu;
    openTaskKey = taskKey(options);
    trigger.setAttribute('aria-expanded', 'true');
  });
  cancel.addEventListener('click', async () => {
    const options = states.get(wrapper);
    if (!options?.task || options.task.status !== '生成中') return;
    setCancelling(wrapper, true);
    try {
      const response = await fetch(
        `${options.apiBaseUrl}/games/${encodeURIComponent(options.game.id)}/nodes/${encodeURIComponent(options.node.id)}/video/cancel`,
        { method: 'POST' },
      );
      if (!response.ok) throw new Error(await readError(response));
      const task = await response.json() as GameTask & { provider_cancel_error?: string };
      options.game.tasks = [...(options.game.tasks || []).filter(item => item.id !== task.id), task];
      options.node.status = task.status;
      closeMenu();
      options.toast(task.provider_cancel_error
        ? `节点视频已在本地取消；服务商取消请求失败：${task.provider_cancel_error}`
        : '已取消节点视频生成');
      await options.onCancelled();
    } catch (error) {
      setCancelling(wrapper, false);
      options.toast(`取消节点视频生成失败：${error instanceof Error ? error.message : '请稍后重试'}`);
    }
  });
}

/** Render the short-drama-style split control only while this selected node has an active video task. */
export function syncGameNodeVideoCancellation(options: Options) {
  const generate = document.querySelector<HTMLButtonElement>('#node-generate');
  if (!generate) return;
  let wrapper = generate.closest<HTMLElement>('[data-game-node-video-generation-actions]');
  if (!wrapper) {
    wrapper = document.createElement('div');
    wrapper.className = 'game-node-video-generation-actions';
    wrapper.dataset.gameNodeVideoGenerationActions = 'true';
    generate.replaceWith(wrapper);
    wrapper.append(generate);
    wrapper.insertAdjacentHTML(
      'beforeend',
      '<button type="button" class="primary game-node-video-cancel-toggle" data-game-node-video-cancel-toggle aria-label="更多视频生成操作" aria-haspopup="true" aria-expanded="false" hidden></button><div class="game-node-video-cancel-menu" data-game-node-video-cancel-menu hidden><button type="button" data-game-node-cancel-video>取消生成</button></div>',
    );
    bindControls(wrapper);
  }
  const cancellable = options.task?.status === '生成中';
  if (openTaskKey && (!cancellable || taskKey(options) !== openTaskKey)) closeMenu();
  states.set(wrapper, options);
  const trigger = wrapper.querySelector<HTMLButtonElement>('[data-game-node-video-cancel-toggle]');
  const menu = wrapper.querySelector<HTMLElement>('[data-game-node-video-cancel-menu]');
  const keepOpen = cancellable && openTaskKey === taskKey(options);
  if (trigger) {
    trigger.hidden = !cancellable;
    trigger.setAttribute('aria-expanded', String(keepOpen));
  }
  if (menu && keepOpen) {
    menu.removeAttribute('hidden');
    openMenu = menu;
  } else {
    menu?.setAttribute('hidden', '');
  }
  if (!escapeListenerReady) {
    document.addEventListener('keydown', event => { if (event.key === 'Escape') closeMenu(); });
    escapeListenerReady = true;
  }
}
