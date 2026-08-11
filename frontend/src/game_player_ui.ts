/** Playback markup and interactions for a generated interactive game. */

import type { Game, GameEdge, GameNode } from './models.js';

export type GamePlayerSession = {
  id: string;
  game_id: string;
  current_node_id: string;
  status: string;
  path: { edge_id: string; option_text: string; source_node_id?: string; target_node_id?: string }[];
  current_node: GameNode;
  choices: GameEdge[];
};

type PlayerMarkupOptions = {
  game: Game;
  session: GamePlayerSession;
  video: string;
  escapeHtml: (value: unknown) => string;
};

type PlayerHandlers = {
  back: () => void;
  restart: () => void;
  choose: (edgeId: string) => Promise<void>;
};

function optionTextWidth(text: string) {
  return Array.from(text.trim()).reduce((width, character) => width + (/[^\u0000-\u00ff]/.test(character) ? 1 : .55), 0);
}

function choiceMarkup(choices: GameEdge[], escapeHtml: PlayerMarkupOptions['escapeHtml']) {
  return choices.map(edge => `<button class="game-player-choice" data-game-player-choice="${escapeHtml(edge.id)}"><span>${escapeHtml(edge.option_text)}</span></button>`).join('');
}

function outcomeMarkup(session: GamePlayerSession, escapeHtml: PlayerMarkupOptions['escapeHtml']) {
  const body = session.status !== 'active'
    ? '<div class="game-player-ending"><h2>故事已结束</h2><button class="primary" id="game-player-ending-restart">再玩一次</button></div>'
    : `<div class="game-player-choices${session.choices.some(choice => optionTextWidth(choice.option_text) > 20) ? ' is-stacked' : ''}">${choiceMarkup(session.choices, escapeHtml)}</div>`;
  return `<section class="game-player-choice-panel" data-game-player-choice-panel hidden aria-live="polite">${body}</section>`;
}

/** Rebuild the developer route as alternating video-node titles and selected choice text. */
export function gamePlayerDebugPath(game: Game, session: GamePlayerSession) {
  const nodes = game.nodes || [];
  const route: string[] = [];
  let lastNodeId = '';
  const appendNode = (nodeId?: string) => {
    if (!nodeId || nodeId === lastNodeId) return;
    const node = nodes.find(item => item.id === nodeId);
    route.push(node?.title || (nodeId === session.current_node_id ? session.current_node.title : nodeId));
    lastNodeId = nodeId;
  };
  for (const selected of session.path) {
    const edge = game.edges?.find(item => item.id === selected.edge_id);
    const source = selected.source_node_id || edge?.source_node_id;
    const target = selected.target_node_id || edge?.target_node_id;
    appendNode(source);
    route.push(selected.option_text || edge?.option_text || '已选选项');
    appendNode(target);
  }
  if (!route.length) appendNode(nodes.find(node => node.node_type === 'start')?.id || session.current_node_id);
  appendNode(session.current_node_id);
  return route.join(' → ');
}

function debugPathMarkup(game: Game, session: GamePlayerSession, escapeHtml: PlayerMarkupOptions['escapeHtml']) {
  return `<p class="game-player-debug-path"><span>试玩路径：</span>${escapeHtml(gamePlayerDebugPath(game, session))}</p>`;
}

/** Builds the single-column player so choices cannot be selected before the node video finishes. */
export function gamePlayerMarkup({ game, session, video, escapeHtml }: PlayerMarkupOptions) {
  const node = session.current_node;
  const videoMarkup = video
    ? `<video data-game-player-video controls autoplay playsinline src="${escapeHtml(video)}"></video>`
    : `<div class="game-player-video-fallback"><strong>${escapeHtml(node.title)}</strong><p>该节点还没有生成视频。</p></div>`;
  return `<div class="game-player-page"><div class="game-player-topbar"><button class="back" id="game-player-back">← 返回编辑器</button><strong>${escapeHtml(game.name)}</strong><button class="ghost game-player-restart" id="game-player-restart">重新开始</button></div><div class="game-player-layout"><section class="game-player-stage"><div class="game-player-video-wrap">${videoMarkup}${outcomeMarkup(session, escapeHtml)}</div></section>${debugPathMarkup(game, session, escapeHtml)}</div></div>`;
}

/** Delays the choice panel until video completion, while keeping fallback nodes playable. */
export function bindGamePlayer(root: HTMLElement, handlers: PlayerHandlers) {
  const choicePanel = root.querySelector<HTMLElement>('[data-game-player-choice-panel]');
  let revealed = false;
  const revealChoices = () => {
    if (revealed || !choicePanel) return;
    revealed = true;
    choicePanel.hidden = false;
    requestAnimationFrame(() => choicePanel.classList.add('is-visible'));
  };
  const video = root.querySelector<HTMLVideoElement>('[data-game-player-video]');
  if (video) {
    video.addEventListener('ended', revealChoices, { once: true });
    video.addEventListener('error', revealChoices, { once: true });
    if (video.ended) revealChoices();
  } else revealChoices();

  root.querySelector('#game-player-back')?.addEventListener('click', handlers.back);
  root.querySelectorAll<HTMLElement>('#game-player-restart,#game-player-ending-restart').forEach(button => button.addEventListener('click', handlers.restart));
  root.querySelectorAll<HTMLButtonElement>('[data-game-player-choice]').forEach(button => button.addEventListener('click', () => {
    const edgeId = button.dataset.gamePlayerChoice;
    if (!edgeId || button.getAttribute('aria-busy') === 'true') return;
    button.setAttribute('aria-busy', 'true');
    void handlers.choose(edgeId).finally(() => button.removeAttribute('aria-busy'));
  }));
}
