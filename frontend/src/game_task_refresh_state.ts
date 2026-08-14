/** Pure state helpers for the editor's background game-task refresh. */

import type { Game } from './models.js';

/** Poll durable game tasks at a restrained rate while at least one is still running. */
export const GAME_TASK_REFRESH_INTERVAL_MS = 3_000;
export const GAME_STREAM_REFRESH_INTERVAL_MS = 1_000;

/** Refresh streamed screenplay and graph checkpoints promptly without speeding up video polling. */
export function gameTaskRefreshInterval(game: Game) {
  return (game.tasks || []).some(task => task.status === '生成中'
    && ['game_script_expansion', 'game_graph_decomposition'].includes(task.type))
    ? GAME_STREAM_REFRESH_INTERVAL_MS
    : GAME_TASK_REFRESH_INTERVAL_MS;
}

export function gameHasRunningTasks(game: Game) {
  return (game.tasks || []).some(task => task.status === '生成中');
}

/** Detect graph additions or removals, which require a one-time editor rerender. */
export function gameGraphSignature(game: Game) {
  const nodeIds = (game.nodes || []).map(node => node.id).sort().join(',');
  const edgeIds = (game.edges || []).map(edge => edge.id).sort().join(',');
  return `${nodeIds}|${edgeIds}`;
}

function mergeItems<T extends { id: string }>(current: T[] | undefined, latest: T[] | undefined) {
  const currentById = new Map((current || []).map(item => [item.id, item]));
  return (latest || []).map(item => {
    const existing = currentById.get(item.id);
    return existing ? Object.assign(existing, item) : item;
  });
}

/**
 * Apply a polling snapshot without replacing objects captured by active editor handlers.
 *
 * Node, asset, and task identities remain stable when their IDs survive the refresh, so an
 * open inspector can receive new task state without losing an in-progress edit or video player.
 */
export function mergeGameTaskSnapshot(current: Game, latest: Game) {
  const { nodes, edges, assets, tasks, ...fields } = latest;
  Object.assign(current, fields);
  current.nodes = mergeItems(current.nodes, nodes);
  current.edges = mergeItems(current.edges, edges);
  current.assets = mergeItems(current.assets, assets);
  current.tasks = mergeItems(current.tasks, tasks);
}
