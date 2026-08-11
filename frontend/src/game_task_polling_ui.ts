/** Apply game-task polling snapshots without recreating the interactive-game workbench. */

import { syncGameSelectedNode } from './game_materials_ui.js';
import { syncGameCoverUi } from './game_cover_ui.js';
import { syncGamePlaceholderUi } from './game_placeholder_ui.js';
import type { Game, GameTask, VoicePreset } from './models.js';
import { gameNodeTaskIsGenerating } from './game_graph_canvas.js';
import { gameGraphSignature, mergeGameTaskSnapshot } from './game_task_refresh_state.js';

type Runtime = {
  apiBaseUrl: string;
  escapeHtml: (value: unknown) => string;
  resolveMediaUrl: (value?: string | null) => string;
  toast: (message: string) => void;
  setGenerationButtonLoading: (button: HTMLButtonElement, loading: boolean, idleText: string) => void;
  getVoicePresets: () => VoicePreset[];
  loadVoicePresets: () => Promise<void>;
};
type TaskFinder = (game: Game, type: string, resourceId?: string) => GameTask | undefined;

type Options = {
  current: Game;
  latest: Game;
  runtime: Runtime;
  findTask: TaskFinder;
  refresh: () => Promise<void>;
};

function syncStatus(element: HTMLElement | null, status: string) {
  if (!element) return;
  element.classList.toggle('running', status === '生成中');
  element.textContent = status;
}

function syncGraphTaskState(game: Game) {
  const main = document.querySelector('main');
  if (!main) return;
  const nodes = game.nodes || [];
  const summary = main.querySelector<HTMLElement>('.game-canvas-panel .panel-title p');
  if (summary) summary.textContent = `${nodes.length} 个视频节点 · ${game.edges?.length || 0} 条选择边`;
  syncStatus(main.querySelector<HTMLElement>('.game-canvas-panel .panel-title > .status'), game.status);
  const nodesById = new Map(nodes.map(node => [node.id, node]));
  main.querySelectorAll<HTMLElement>('[data-game-node]').forEach(card => {
    const node = nodesById.get(card.dataset.gameNode || '');
    if (!node) return;
    const title = card.querySelector<HTMLElement>('strong');
    const detail = card.querySelector<HTMLElement>('small');
    const generating = gameNodeTaskIsGenerating(game, node);
    if (title) title.textContent = node.title;
    if (detail) detail.textContent = `${node.duration_seconds}s · ${node.status}`;
    card.classList.toggle('is-video-generating', generating);
    card.toggleAttribute('aria-busy', generating);
    const loading = card.querySelector<HTMLElement>('[data-game-node-loading]');
    if (loading) loading.hidden = !generating;
  });
}

/**
 * Merge a durable task refresh and update only the controls whose task state changed.
 *
 * Returning true means graph nodes or edges were added or removed; the caller should then do a
 * one-time full render so the new graph structure receives its interaction bindings.
 */
export function syncGameTaskPollingUi(options: Options) {
  const graphChanged = gameGraphSignature(options.current) !== gameGraphSignature(options.latest);
  mergeGameTaskSnapshot(options.current, options.latest);
  if (graphChanged) return true;
  syncGraphTaskState(options.current);
  syncGameSelectedNode(options.current, options.runtime, options.findTask, options.refresh);
  syncGameCoverUi(options.current);
  syncGamePlaceholderUi(options.current);
  return false;
}
