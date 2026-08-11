/** Keep the game-wide video cancellation button aligned with durable node and serial tasks. */

import type { Game } from './models.js';

type Options = {
  apiBaseUrl: string;
  game: Game;
  reloadGame: (gameId: string) => Promise<void>;
  toast: (message: string) => void;
};

type Result = { cancelled_count: number; provider_cancel_errors?: Array<{ task_id: string; error: string }> };

let current: Options | null = null;

function activeVideoTaskCount(game: Game) {
  const videos = (game.tasks || []).filter(task => task.type === 'node_video_generation' && task.status === '生成中').length;
  const serial = (game.tasks || []).some(task => task.type === 'serial_game_node_video_batch' && task.status === '生成中');
  return videos || serial ? Math.max(videos, 1) : 0;
}

async function readError(response: Response) {
  const payload = await response.json().catch(() => ({})) as { detail?: unknown };
  return typeof payload.detail === 'string' ? payload.detail : `HTTP ${response.status}`;
}

/** Bind the editor toolbar action while keeping provider cancellation best-effort and local state durable. */
export function syncGameBatchVideoCancellation(options: Options) {
  current = options;
  const button = document.querySelector<HTMLButtonElement>('#game-cancel-all-videos');
  if (!button) return;
  if (button.dataset.cancelling !== 'true') {
    const active = activeVideoTaskCount(options.game);
    button.disabled = active === 0;
    button.title = active ? `取消 ${active} 个进行中的视频任务` : '当前没有进行中的视频任务';
  }
  if (button.dataset.batchCancellationBound === 'true') return;
  button.dataset.batchCancellationBound = 'true';
  button.addEventListener('click', async () => {
    const request = current;
    if (!request || button.disabled) return;
    button.dataset.cancelling = 'true'; button.disabled = true; button.textContent = '取消中…';
    try {
      const response = await fetch(`${request.apiBaseUrl}/games/${encodeURIComponent(request.game.id)}/videos/cancel`, { method: 'POST' });
      if (!response.ok) throw new Error(await readError(response));
      const result = await response.json() as Result;
      const errors = result.provider_cancel_errors?.length || 0;
      request.toast(errors ? `已取消 ${result.cancelled_count} 个视频任务；${errors} 个远端任务取消失败` : `已取消 ${result.cancelled_count} 个视频任务`);
      await request.reloadGame(request.game.id);
    } catch (error) {
      button.dataset.cancelling = 'false'; button.textContent = '取消所有视频任务'; button.disabled = false;
      request.toast(`取消所有视频任务失败：${error instanceof Error ? error.message : '请稍后重试'}`);
    }
  });
}
