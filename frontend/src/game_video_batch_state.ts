/** Pure durable-state helpers shared by the interactive-game video batch controls and tests. */

import type { Game } from './models.js';

const SERIAL_VIDEO_BATCH = 'serial_game_node_video_batch';

/** Read progress only while the persisted serial coordinator remains active. */
export function serialGameBatchProgress(game: Game): { completed: number; total: number } | null {
  const task = (game.tasks || []).find(item => item.type === SERIAL_VIDEO_BATCH && item.status === '生成中');
  if (!task) return null;
  const snapshot = task.input_snapshot || {};
  const number = (value: unknown) => Number.isFinite(Number(value)) ? Number(value) : 0;
  return { completed: number(snapshot.completed_count), total: number(snapshot.total_count) };
}
