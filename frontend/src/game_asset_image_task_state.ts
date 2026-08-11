/** Local task-state updates for image generation started from the game material drawer. */
import type { Game, GameTask } from './models.js';

/**
 * Mirror a newly queued image task locally so the open drawer can immediately render its loading state.
 * The regular game refresh later replaces this optimistic state with the durable task record.
 */
export function applyQueuedGameImageTask(game: Game, task: GameTask) {
  game.tasks = [...(game.tasks || []).filter(item => item.id !== task.id), task];
  if (task.type === 'game_asset_image') {
    const asset = (game.assets || []).find(item => item.id === task.resource_id);
    if (asset) asset.status = task.status;
  }
  if (task.type === 'game_asset_variant_image') {
    const variant = (game.assets || []).flatMap(asset => asset.variants || []).find(item => item.id === task.resource_id);
    if (variant) variant.status = task.status;
  }
}
