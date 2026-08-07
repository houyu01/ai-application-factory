/** Add the shot-video cancellation split button and keep it in sync with tasks. */
import type { ApiProject, DramaShot, GenerationTask } from './models.js';

type VideoCancellationOptions = {
  apiBaseUrl: string;
  project: ApiProject;
  shot: DramaShot;
  task?: GenerationTask;
  toast: (message: string) => void;
  onCancelled?: () => Promise<void> | void;
};

type VideoCancellationRuntime = {
  apiBaseUrl: string;
  getActiveProject: () => ApiProject | null;
  getSelectedShot: (project: ApiProject) => DramaShot | undefined;
  getVideoTask: (project: ApiProject, shotId: string) => GenerationTask | undefined;
  toast: (message: string) => void;
  reloadProject: (projectId: string) => Promise<void>;
};

const states = new WeakMap<HTMLElement, VideoCancellationOptions>();
let openMenu: HTMLElement | null = null;
let openMenuTaskKey: string | null = null;
let escapeListenerReady = false;
let renderedGenerateButton: HTMLButtonElement | null = null;
let runtime: VideoCancellationRuntime | null = null;
let observerReady = false;

function taskKey(options: VideoCancellationOptions, task = options.task) {
  return task?.id
    ? `${options.project.id}:${options.shot.id}:${task.id}`
    : null;
}

function shotKey(options: VideoCancellationOptions) {
  return `${options.project.id}:${options.shot.id}`;
}

function openMenuBelongsTo(options: VideoCancellationOptions) {
  return Boolean(openMenuTaskKey?.startsWith(`${shotKey(options)}:`));
}

function closeMenu() {
  openMenu?.parentElement
    ?.querySelector<HTMLButtonElement>('[data-drama-video-cancel-toggle]')
    ?.setAttribute('aria-expanded', 'false');
  openMenu?.setAttribute('hidden', '');
  openMenu = null;
  openMenuTaskKey = null;
}

function closeMenuOnEscape(event: KeyboardEvent) {
  if (event.key !== 'Escape' || !openMenu) return;
  closeMenu();
}

function readError(response: Response) {
  return response.json()
    .then(payload => typeof payload?.detail === 'string' ? payload.detail : `HTTP ${response.status}`)
    .catch(() => `HTTP ${response.status}`);
}

function updateLocalTask(options: VideoCancellationOptions, task: GenerationTask) {
  options.project.tasks = [
    ...(options.project.tasks || []).filter(item => item.id !== task.id),
    task,
  ];
  options.shot.status = task.status;
}

function cancelButtonState(wrapper: HTMLElement, cancelling: boolean) {
  const button = wrapper.querySelector<HTMLButtonElement>('[data-drama-cancel-video]');
  const trigger = wrapper.querySelector<HTMLButtonElement>('[data-drama-video-cancel-toggle]');
  if (button) {
    button.disabled = cancelling;
    button.textContent = cancelling ? '取消中…' : '取消生成';
  }
  if (trigger) trigger.disabled = cancelling;
}

function bindControls(wrapper: HTMLElement) {
  const trigger = wrapper.querySelector<HTMLButtonElement>('[data-drama-video-cancel-toggle]');
  const menu = wrapper.querySelector<HTMLElement>('[data-drama-video-cancel-menu]');
  const cancel = wrapper.querySelector<HTMLButtonElement>('[data-drama-cancel-video]');
  if (!trigger || !menu || !cancel) return;
  // This menu is state-driven, not focus-driven: moving focus to the arrow or
  // its action must not make a background refresh close the popup.
  trigger.addEventListener('pointerdown', event => event.stopPropagation());
  menu.addEventListener('pointerdown', event => event.stopPropagation());
  menu.addEventListener('click', event => event.stopPropagation());
  trigger.addEventListener('click', event => {
    event.stopPropagation();
    const opening = menu.hasAttribute('hidden');
    closeMenu();
    if (opening) {
      const options = states.get(wrapper);
      if (!options?.task || options.task.status !== '生成中') return;
      menu.removeAttribute('hidden');
      openMenu = menu;
      openMenuTaskKey = taskKey(options);
      trigger.setAttribute('aria-expanded', 'true');
    }
  });
  cancel.addEventListener('click', async () => {
    const options = states.get(wrapper);
    if (!options?.task || options.task.status !== '生成中') return;
    cancelButtonState(wrapper, true);
    try {
      const response = await fetch(
        `${options.apiBaseUrl}/projects/${encodeURIComponent(options.project.id)}/shots/${encodeURIComponent(options.shot.id)}/video/cancel`,
        { method: 'POST' },
      );
      if (!response.ok) throw new Error(await readError(response));
      const task = await response.json() as GenerationTask & { provider_cancel_error?: string };
      updateLocalTask(options, task);
      closeMenu();
      if (task.provider_cancel_error) {
        options.toast(`视频已在本地取消；服务商取消请求失败：${task.provider_cancel_error}`);
      } else {
        options.toast('视频生成已取消');
      }
      await options.onCancelled?.();
    } catch (error) {
      cancelButtonState(wrapper, false);
      options.toast(`取消视频生成失败：${error instanceof Error ? error.message : '请稍后重试'}`);
    }
  });
}

/** Render a cancellable arrow only while the selected shot has a durable video task. */
export function syncDramaVideoCancellation(options: VideoCancellationOptions) {
  const generate = document.querySelector<HTMLButtonElement>('#drama-generate-shot-video');
  if (!generate) return;
  let wrapper = generate.closest<HTMLElement>('[data-drama-video-generation-actions]');
  if (!wrapper) {
    wrapper = document.createElement('div');
    wrapper.className = 'drama-video-generation-actions';
    wrapper.dataset.dramaVideoGenerationActions = 'true';
    generate.replaceWith(wrapper);
    wrapper.append(generate);
    wrapper.insertAdjacentHTML(
      'beforeend',
      '<button type="button" class="primary drama-video-cancel-toggle" data-drama-video-cancel-toggle aria-label="更多视频生成操作" aria-haspopup="true" aria-expanded="false" hidden></button><div class="drama-video-cancel-menu" data-drama-video-cancel-menu hidden><button type="button" data-drama-cancel-video>取消生成</button></div>',
    );
    bindControls(wrapper);
  }
  const previous = states.get(wrapper);
  const cancellable = options.task?.status === '生成中';
  // The arrow is deliberately driven by the latest task state, rather than a
  // cached menu state. Once the task is absent or terminal it vanishes at once.
  if (
    openMenuTaskKey
    && (!openMenuBelongsTo(options) || !cancellable || taskKey(options) !== openMenuTaskKey)
  ) closeMenu();
  states.set(wrapper, {
    ...options,
    onCancelled: options.onCancelled || previous?.onCancelled,
  });
  const trigger = wrapper.querySelector<HTMLButtonElement>('[data-drama-video-cancel-toggle]');
  const menu = wrapper.querySelector<HTMLElement>('[data-drama-video-cancel-menu]');
  const shouldKeepMenuOpen = cancellable && openMenuTaskKey === taskKey(options);
  if (trigger) {
    trigger.hidden = !cancellable;
    trigger.setAttribute('aria-expanded', String(shouldKeepMenuOpen));
  }
  if (menu && shouldKeepMenuOpen) {
    menu.removeAttribute('hidden');
    openMenu = menu;
  } else if (!shouldKeepMenuOpen) {
    menu?.setAttribute('hidden', '');
  }
  if (!escapeListenerReady) {
    document.addEventListener('keydown', closeMenuOnEscape);
    escapeListenerReady = true;
  }
}

/** Sync the split control after the editor has applied a durable task update. */
export function refreshDramaVideoCancellation(
  projectOverride?: ApiProject,
  shotOverride?: DramaShot,
) {
  const project = projectOverride || runtime?.getActiveProject();
  const shot = shotOverride || (project && runtime?.getSelectedShot(project));
  if (!project || !shot || !runtime) return;
  syncDramaVideoCancellation({
    apiBaseUrl: runtime.apiBaseUrl,
    project,
    shot,
    task: runtime.getVideoTask(project, shot.id),
    toast: runtime.toast,
    onCancelled: () => runtime?.reloadProject(project.id),
  });
  renderedGenerateButton = document.querySelector<HTMLButtonElement>('#drama-generate-shot-video');
}

function rehydrateCancellationAfterButtonReplacement() {
  const generate = document.querySelector<HTMLButtonElement>('#drama-generate-shot-video');
  if (!generate) {
    renderedGenerateButton = null;
    closeMenu();
    return;
  }
  if (generate === renderedGenerateButton) return;
  renderedGenerateButton = generate;
  if (generate) refreshDramaVideoCancellation();
}

/** Configure automatic rendering whenever the drama editor updates its task UI. */
export function configureDramaVideoCancellation(value: VideoCancellationRuntime) {
  runtime = value;
  if (!observerReady) {
    const app = document.querySelector('#app');
    if (app) {
      new MutationObserver(rehydrateCancellationAfterButtonReplacement).observe(app, {
        childList: true,
        subtree: true,
      });
      observerReady = true;
    }
  }
  refreshDramaVideoCancellation();
}
