/** Synchronize the video-preview panel with the current shot's durable task. */
import type { ApiProject, DramaShot, GenerationTask } from './models.js';
import { icon } from './ui_icons.js';

function taskDetail(task?: GenerationTask) {
  if (!task) return '正在提交视频生成任务…';
  const progress = Math.max(0, Math.min(100, Number(task.progress || 0)));
  return task.stage ? `生成中 ${progress}% · ${task.stage.replace(/^provider_/, '')}` : `生成中 ${progress}%`;
}

/** Render an in-place preview state while a selected shot is producing a video. */
export function syncDramaVideoPreviewStatus(
  panel: HTMLElement | null | undefined,
  shot: DramaShot,
  task?: GenerationTask,
) {
  if (!panel) return;
  const running = task?.status === '生成中';
  const placeholder = panel.querySelector<HTMLElement>('.drama-video-placeholder');
  const existingNotice = panel.querySelector<HTMLElement>('[data-drama-video-generation-notice]');
  if (!running) {
    existingNotice?.remove();
    return;
  }

  const detail = taskDetail(task);
  if (placeholder) {
    placeholder.classList.add('is-generating');
    placeholder.setAttribute('role', 'status');
    placeholder.innerHTML = '<span class="generation-spinner" aria-hidden="true"></span><strong>视频生成中</strong><span data-drama-video-generation-detail></span>';
    placeholder.querySelector<HTMLElement>('[data-drama-video-generation-detail]')!.textContent = detail;
    return;
  }

  const notice = existingNotice || document.createElement('div');
  notice.className = 'drama-video-generation-notice';
  notice.dataset.dramaVideoGenerationNotice = 'true';
  notice.setAttribute('role', 'status');
  notice.innerHTML = '<span class="generation-spinner" aria-hidden="true"></span><span data-drama-video-generation-detail></span>';
  notice.querySelector<HTMLElement>('[data-drama-video-generation-detail]')!.textContent = detail;
  if (!existingNotice) panel.querySelector('.panel-title')?.insertAdjacentElement('afterend', notice);
}

/** Add cross-episode previous/next shot controls beside the video-preview title. */
export function syncDramaVideoPreviewNavigation(
  panel: HTMLElement | null | undefined,
  project: ApiProject,
  shot: DramaShot,
  navigate: (shotId: string) => void,
) {
  const title = panel?.querySelector<HTMLElement>('.drama-video-panel .panel-title');
  const heading = title?.querySelector<HTMLElement>(':scope > div');
  const status = title?.querySelector<HTMLElement>(':scope > .status');
  if (!title || !heading) return;
  heading.querySelector('p')?.remove();
  heading.classList.add('drama-video-heading');
  if (status) {
    const currentStatus = shot.status || '未生成';
    status.className = `drama-video-status-indicator ${currentStatus === '生成中' ? 'running' : currentStatus === '生成失败' ? 'failed' : currentStatus === '生成成功' ? 'success' : ''}`;
    status.dataset.dramaVideoStatusIndicator = 'true';
    status.setAttribute('role', 'img');
    status.setAttribute('aria-label', currentStatus);
    status.title = currentStatus;
    status.innerHTML = icon('info');
    heading.append(status);
  }
  const shots = project.shots || [];
  const index = shots.findIndex(item => item.id === shot.id);
  const previous = index > 0 ? shots[index - 1] : undefined;
  const next = index >= 0 && index < shots.length - 1 ? shots[index + 1] : undefined;
  const navigation = title.querySelector<HTMLElement>('[data-drama-video-shot-navigation]') || document.createElement('div');
  navigation.className = 'drama-video-shot-navigation';
  navigation.dataset.dramaVideoShotNavigation = 'true';
  navigation.setAttribute('aria-label', '切换分镜');
  navigation.innerHTML = `<button type="button" class="ghost compact"${previous ? '' : ' disabled'}>← 上一个分镜</button><button type="button" class="ghost compact"${next ? '' : ' disabled'}>下一个分镜 →</button>`;
  if (!navigation.parentElement) title.append(navigation);
  const [previousButton, nextButton] = navigation.querySelectorAll<HTMLButtonElement>('button');
  if (previousButton && previous) previousButton.addEventListener('click', () => navigate(previous.id));
  if (nextButton && next) nextButton.addEventListener('click', () => navigate(next.id));
}
