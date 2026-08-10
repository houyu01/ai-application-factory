import type { DramaShot, GenerationTask } from './models.js';
import { latestDramaVideoGeneration } from './drama_video_history.js';

/** Show preview info for the latest video generation, without surfacing stale historical failures. */
export function syncDramaVideoGenerationInfo(
  videoPanel: HTMLElement | null | undefined,
  shot: DramaShot,
  videoTask: GenerationTask | undefined,
  escapeHtml: (value: string) => string,
) {
  const latest = latestDramaVideoGeneration(shot, videoTask);
  const status = latest?.status;
  const isSuccess = status === '生成成功';
  const isFailure = status === '生成失败';
  const title = isSuccess ? '视频生成成功' : isFailure ? '视频生成失败' : '';
  const message = isSuccess
    ? '当前视频已生成成功，可在预览区播放。'
    : isFailure
      ? latest?.error?.trim() || '视频生成失败，请稍后重试。'
      : '';
  const indicator = videoPanel?.querySelector<HTMLElement>('[data-drama-video-status-indicator]');
  const existing = indicator?.querySelector<HTMLElement>('[data-drama-video-generation-tooltip]');
  indicator?.classList.remove('drama-video-failure-indicator');
  if (!indicator || !message) {
    existing?.remove();
    indicator?.classList.remove('has-drama-video-generation-info');
    indicator?.removeAttribute('aria-describedby');
    indicator?.removeAttribute('tabindex');
    return;
  }
  indicator.classList.add('has-drama-video-generation-info');
  indicator.setAttribute('aria-label', `${title}，悬浮查看详情`);
  indicator.setAttribute('aria-describedby', 'drama-video-generation-tooltip');
  indicator.tabIndex = 0;
  const tooltip = existing || document.createElement('span');
  tooltip.className = `drama-video-generation-tooltip ${isSuccess ? 'success' : 'failed'}`;
  tooltip.dataset.dramaVideoGenerationTooltip = 'true';
  tooltip.id = 'drama-video-generation-tooltip';
  tooltip.setAttribute('role', 'tooltip');
  tooltip.innerHTML = `<strong>${title}</strong><span>${escapeHtml(message)}</span>`;
  if (!existing) indicator.append(tooltip);
}
