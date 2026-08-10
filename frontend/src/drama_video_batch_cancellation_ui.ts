/** Keep the project-wide video cancellation control aligned with durable task state. */
import type { ApiProject } from './models.js';

type BatchCancellationOptions = {
  apiBaseUrl: string;
  project: ApiProject;
  toast: (message: string) => void;
  reloadProject: (projectId: string) => Promise<void>;
};

type BatchCancellationResult = {
  cancelled_count: number;
  provider_cancel_errors?: Array<{ task_id: string; error: string }>;
};

let current: BatchCancellationOptions | null = null;

function activeVideoTaskCount(project: ApiProject) {
  const videoCount = (project.tasks || []).filter(task => task.type === 'shot_video' && task.status === '生成中').length;
  const serialActive = (project.tasks || []).some(task => task.type === 'serial_shot_video_batch' && task.status === '生成中');
  return videoCount || serialActive ? Math.max(videoCount, 1) : 0;
}

async function readError(response: Response) {
  const payload = await response.json().catch(() => ({})) as { detail?: unknown };
  return typeof payload.detail === 'string' ? payload.detail : `HTTP ${response.status}`;
}

/** Bind the top-bar cancellation button without sharing prompt or image task state. */
export function syncDramaBatchVideoCancellation(options: BatchCancellationOptions) {
  current = options;
  const button = document.querySelector<HTMLButtonElement>('#drama-cancel-all-videos');
  if (!button) return;
  if (button.dataset.cancelling !== 'true') {
    const activeCount = activeVideoTaskCount(options.project);
    button.disabled = activeCount === 0;
    button.title = activeCount ? `取消 ${activeCount} 个进行中的视频任务` : '当前没有进行中的视频任务';
  }
  if (button.dataset.batchCancellationBound === 'true') return;
  button.dataset.batchCancellationBound = 'true';
  button.addEventListener('click', async () => {
    const request = current;
    if (!request || button.disabled) return;
    button.dataset.cancelling = 'true';
    button.disabled = true;
    button.textContent = '取消中…';
    try {
      const response = await fetch(
        `${request.apiBaseUrl}/projects/${encodeURIComponent(request.project.id)}/videos/cancel`,
        { method: 'POST' },
      );
      if (!response.ok) throw new Error(await readError(response));
      const result = await response.json() as BatchCancellationResult;
      const providerErrors = result.provider_cancel_errors?.length || 0;
      request.toast(providerErrors
        ? `已取消 ${result.cancelled_count} 个视频任务；${providerErrors} 个远端任务取消失败`
        : `已取消 ${result.cancelled_count} 个视频任务`);
      await request.reloadProject(request.project.id);
    } catch (error) {
      button.dataset.cancelling = 'false';
      button.textContent = '取消所有视频任务';
      button.disabled = false;
      request.toast(`取消所有视频任务失败：${error instanceof Error ? error.message : '请稍后重试'}`);
    }
  });
}
