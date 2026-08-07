import type { DramaShot, GenerationTask } from './models.js';

export function syncDramaVideoFailure(
  videoPanel: HTMLElement | null | undefined,
  shot: DramaShot,
  videoTask: GenerationTask | undefined,
  escapeHtml: (value: string) => string,
) {
  const failedVersion = [...(shot.versions || [])]
    .reverse()
    .find(version => version.status === '生成失败' && version.error_message);
  const currentFailure = videoTask?.status === '生成失败'
    || (!videoTask && shot.status === '生成失败');
  const message = currentFailure
    ? videoTask?.error_message?.trim() || failedVersion?.error_message?.trim() || '视频生成失败，请稍后重试。'
    : '';
  const indicator = videoPanel?.querySelector<HTMLElement>('[data-drama-video-status-indicator]');
  const existing = indicator?.querySelector<HTMLElement>('[data-drama-video-failure-tooltip]');
  if (!indicator || !message) {
    existing?.remove();
    indicator?.classList.remove('drama-video-failure-indicator');
    indicator?.removeAttribute('aria-describedby');
    indicator?.removeAttribute('tabindex');
    return;
  }
  indicator.classList.add('drama-video-failure-indicator');
  indicator.setAttribute('aria-label', '视频生成失败，悬浮查看原因');
  indicator.setAttribute('aria-describedby', 'drama-video-failure-tooltip');
  indicator.tabIndex = 0;
  if (!existing) indicator.insertAdjacentHTML('beforeend', `<span class="drama-video-failure-tooltip" data-drama-video-failure-tooltip id="drama-video-failure-tooltip" role="tooltip"><strong>视频生成失败</strong><span>${escapeHtml(message)}</span></span>`);
}
