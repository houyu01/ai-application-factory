/** Interactive DAG canvas for positioning video nodes and creating choice edges. */

import type { Game, GameEdge, GameNode } from './models.js';

const NODE_WIDTH = 180;
const NODE_HEIGHT = 92;
const MIN_SCALE = 0.35;
const MAX_SCALE = 1.8;
const GRAPH_ORIGIN = 100_000;
const MIN_STAGE_SIZE = GRAPH_ORIGIN * 2;
const graphViews = new Map<string, GraphView>();

type GraphView = { scale: number; panX: number; panY: number };
type CanvasOptions = {
  game: Game;
  apiBaseUrl: string;
  escapeHtml: (value: unknown) => string;
  toast: (message: string) => void;
  selectNode: (nodeId: string) => void;
  selectEdge: (edgeId: string) => void;
  createEdge: (sourceNodeId: string, targetNodeId: string) => void;
  reload: () => Promise<unknown>;
};
type Point = { x: number; y: number };

/** Keeps the viewport grid aligned with the transformed graph at every pan and zoom level. */
export function gameGraphGridStyle(scale: number, panX: number, panY: number) {
  return { size: `${24 * scale}px`, x: `${panX}px`, y: `${panY}px` };
}

/** Identify nodes with an active prompt or video task, the only states that need a canvas loader. */
export function gameNodeTaskIsGenerating(game: Game, node: GameNode) {
  return (game.tasks || []).some(task => ['game_node_prompt', 'node_video_generation'].includes(task.type) && task.resource_id === node.id && task.status === '生成中');
}

export function gameGraphCanvasMarkup(game: Game, escapeHtml: CanvasOptions['escapeHtml']) {
  const nodes = game.nodes || [];
  const edges = game.edges || [];
  const width = Math.max(MIN_STAGE_SIZE, ...nodes.map(node => node.position_x + GRAPH_ORIGIN + 260));
  const height = Math.max(MIN_STAGE_SIZE, ...nodes.map(node => node.position_y + GRAPH_ORIGIN + 180));
  const nodeMap = new Map(nodes.map(node => [node.id, node]));
  const offsets = edgeOffsets(edges);
  const edgeLines = edges.map(edge => {
    const source = nodeMap.get(edge.source_node_id);
    const target = nodeMap.get(edge.target_node_id);
    if (!source || !target) return '';
    const shape = edgeShape(source, target, offsets.get(edge.id) || 0);
    return `<path class="game-edge-line" data-game-edge="${escapeHtml(edge.id)}" data-source-node="${escapeHtml(edge.source_node_id)}" data-target-node="${escapeHtml(edge.target_node_id)}" data-edge-offset="${shape.offset}" d="${shape.d}"></path>`;
  }).join('');
  const labels = edges.map(edge => {
    const source = nodeMap.get(edge.source_node_id);
    const target = nodeMap.get(edge.target_node_id);
    if (!source || !target) return '';
    const shape = edgeShape(source, target, offsets.get(edge.id) || 0);
    return `<button type="button" class="game-edge-label" data-game-edge="${escapeHtml(edge.id)}" style="left:${shape.label.x + GRAPH_ORIGIN}px;top:${shape.label.y + GRAPH_ORIGIN}px" title="编辑选项：${escapeHtml(edge.option_text)}">${escapeHtml(edge.option_text)}</button>`;
  }).join('');
  const cards = nodes.map(node => {
    const generating = gameNodeTaskIsGenerating(game, node);
    return `<button type="button" class="game-node ${escapeHtml(node.node_type)}${generating ? ' is-video-generating' : ''}" data-game-node="${escapeHtml(node.id)}"${generating ? ' aria-busy="true"' : ''} style="left:${node.position_x + GRAPH_ORIGIN}px;top:${node.position_y + GRAPH_ORIGIN}px"><span class="game-node-type">${nodeTypeLabel(node.node_type)}</span><strong>${escapeHtml(node.title)}</strong><small>${node.duration_seconds}s · ${escapeHtml(node.status)}</small><span class="game-node-video-loading" data-game-node-loading aria-hidden="true"${generating ? '' : ' hidden'}><span class="generation-spinner"></span></span><span class="game-node-link-handle" data-game-link-source="${escapeHtml(node.id)}" title="拖到另一视频节点以新增选项" aria-label="从此节点创建选项">+</span></button>`;
  }).join('');
  return `<div class="game-graph-canvas" data-game-graph-canvas="${escapeHtml(game.id)}"><div class="game-graph-toolbar"><span class="game-graph-help">拖动空白处平移 · 滚轮缩放 · 拖动节点右侧 + 连线</span><div class="game-graph-toolbar-actions"><div class="game-graph-zoom-controls"><button type="button" class="ghost compact" data-game-zoom-out aria-label="缩小画布">−</button><span data-game-zoom-label>100%</span><button type="button" class="ghost compact" data-game-zoom-in aria-label="放大画布">＋</button><button type="button" class="ghost compact" data-game-fit>适应画布</button></div><button type="button" class="ghost compact" data-game-expand-canvas aria-label="全屏展开画布">⛶ 全屏画布</button><button type="button" class="ghost compact" data-game-add-edge>新增选项</button></div></div><div class="game-graph-viewport" data-game-graph-viewport><div class="game-graph" data-game-graph-stage data-game-graph-origin="${GRAPH_ORIGIN}" style="width:${width}px;height:${height}px"><svg class="game-edges" width="${width}" height="${height}" viewBox="-${GRAPH_ORIGIN} -${GRAPH_ORIGIN} ${width} ${height}" aria-hidden="true"><defs><marker id="game-edge-arrow" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z"></path></marker></defs>${edgeLines}</svg>${labels}${cards}</div></div></div>`;
}

export function bindGameGraphCanvas(options: CanvasOptions) {
  const root = document.querySelector<HTMLElement>(`[data-game-graph-canvas="${options.game.id}"]`);
  const viewport = root?.querySelector<HTMLElement>('[data-game-graph-viewport]');
  const stage = root?.querySelector<HTMLElement>('[data-game-graph-stage]');
  if (!root || !viewport || !stage) return;
  const origin = Number(stage.dataset.gameGraphOrigin || 0);
  const existing = graphViews.get(options.game.id);
  const view = existing || { scale: 1, panX: 24, panY: 24 };
  graphViews.set(options.game.id, view);
  const renderView = () => {
    stage.style.transform = `translate(${view.panX - origin * view.scale}px, ${view.panY - origin * view.scale}px) scale(${view.scale})`;
    const grid = gameGraphGridStyle(view.scale, view.panX, view.panY);
    viewport.style.setProperty('--game-grid-size', grid.size);
    viewport.style.setProperty('--game-grid-x', grid.x);
    viewport.style.setProperty('--game-grid-y', grid.y);
    const label = root.querySelector<HTMLElement>('[data-game-zoom-label]');
    if (label) label.textContent = `${Math.round(view.scale * 100)}%`;
  };
  const fit = () => {
    const nodes = options.game.nodes || [];
    if (!nodes.length) return;
    const bounds = nodeBounds(nodes);
    const available = viewport.getBoundingClientRect();
    const scale = clamp(Math.min((available.width - 72) / bounds.width, (available.height - 72) / bounds.height), MIN_SCALE, MAX_SCALE);
    view.scale = scale;
    view.panX = (available.width - bounds.width * scale) / 2 - bounds.x * scale;
    view.panY = (available.height - bounds.height * scale) / 2 - bounds.y * scale;
    renderView();
  };
  if (!existing) requestAnimationFrame(fit);
  renderView();
  root.querySelector('[data-game-zoom-out]')?.addEventListener('click', () => zoomAround(viewport, view, 0.82, renderView));
  root.querySelector('[data-game-zoom-in]')?.addEventListener('click', () => zoomAround(viewport, view, 1.22, renderView));
  root.querySelector('[data-game-fit]')?.addEventListener('click', fit);
  root.querySelector('[data-game-add-edge]')?.addEventListener('click', () => options.createEdge('', ''));
  root.querySelector('[data-game-expand-canvas]')?.addEventListener('click', () => openGameGraphFullscreen(root, renderView));
  viewport.addEventListener('wheel', event => {
    event.preventDefault();
    const point = pointInViewport(event.clientX, event.clientY, viewport);
    const before = worldPoint(point, view);
    view.scale = clamp(view.scale * (event.deltaY < 0 ? 1.12 : 0.89), MIN_SCALE, MAX_SCALE);
    view.panX = point.x - before.x * view.scale;
    view.panY = point.y - before.y * view.scale;
    renderView();
  }, { passive: false });
  bindCanvasPan(viewport, view, renderView);
  bindNodeInteractions(stage, viewport, view, options, renderView);
  bindLinkInteractions(stage, viewport, view, options);
  stage.querySelectorAll<HTMLElement>('[data-game-edge]').forEach(item => item.addEventListener('click', event => {
    event.stopPropagation();
    options.selectEdge(item.dataset.gameEdge || '');
  }));
}

/** Moves the active graph into a focus overlay without duplicating its interaction state. */
function openGameGraphFullscreen(canvas: HTMLElement, render: () => void) {
  const parent = canvas.parentNode;
  if (!parent) return;
  const anchor = document.createComment('game-graph-canvas-anchor');
  parent.insertBefore(anchor, canvas);
  const overlay = document.createElement('div');
  overlay.className = 'modal-backdrop game-graph-fullscreen-backdrop';
  overlay.innerHTML = '<section class="game-graph-fullscreen" role="dialog" aria-modal="true" aria-label="全屏分支编辑画布"><header class="game-graph-fullscreen-head"><div><h2>分支编辑画布</h2><p>拖动节点和选项边，专注整理分支路径。</p></div><button type="button" class="ghost compact" data-game-close-fullscreen>退出全屏</button></header><div class="game-graph-fullscreen-body"></div></section>';
  const mount = overlay.querySelector<HTMLElement>('.game-graph-fullscreen-body');
  if (!mount) return;
  mount.append(canvas);
  document.body.append(overlay);
  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    window.removeEventListener('keydown', onKeydown);
    anchor.replaceWith(canvas);
    overlay.remove();
    requestAnimationFrame(render);
  };
  const onKeydown = (event: KeyboardEvent) => { if (event.key === 'Escape') close(); };
  overlay.addEventListener('click', event => { if (event.target === overlay) close(); });
  overlay.querySelector('[data-game-close-fullscreen]')?.addEventListener('click', close);
  window.addEventListener('keydown', onKeydown);
  requestAnimationFrame(render);
}

function bindCanvasPan(viewport: HTMLElement, view: GraphView, render: () => void) {
  viewport.addEventListener('pointerdown', event => {
    if (event.button !== 0 || (event.target as HTMLElement).closest('[data-game-node],[data-game-edge]')) return;
    const start = { x: event.clientX, y: event.clientY, panX: view.panX, panY: view.panY };
    viewport.classList.add('is-panning');
    const move = (next: PointerEvent) => { view.panX = start.panX + next.clientX - start.x; view.panY = start.panY + next.clientY - start.y; render(); };
    const stop = () => { viewport.classList.remove('is-panning'); window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', stop); };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', stop, { once: true });
  });
}

function bindNodeInteractions(stage: HTMLElement, viewport: HTMLElement, view: GraphView, options: CanvasOptions, render: () => void) {
  stage.querySelectorAll<HTMLElement>('[data-game-node]').forEach(card => {
    card.addEventListener('pointerdown', event => {
      if (event.button !== 0 || (event.target as HTMLElement).closest('[data-game-link-source]')) return;
      event.stopPropagation();
      const node = (options.game.nodes || []).find(item => item.id === card.dataset.gameNode);
      if (!node) return;
      const start = { x: event.clientX, y: event.clientY, nodeX: node.position_x, nodeY: node.position_y };
      let moved = false;
      card.classList.add('is-dragging');
      const move = (next: PointerEvent) => {
        const x = Math.round(start.nodeX + (next.clientX - start.x) / view.scale);
        const y = Math.round(start.nodeY + (next.clientY - start.y) / view.scale);
        moved ||= Math.abs(next.clientX - start.x) > 3 || Math.abs(next.clientY - start.y) > 3;
        node.position_x = x;
        node.position_y = y;
        card.style.left = `${node.position_x + graphOrigin(stage)}px`;
        card.style.top = `${node.position_y + graphOrigin(stage)}px`;
        updateEdges(stage);
      };
      const stop = () => {
        card.classList.remove('is-dragging');
        window.removeEventListener('pointermove', move);
        if (moved) {
          card.dataset.gameDragged = 'true';
          void persistNodePosition(options, node);
        }
      };
      window.addEventListener('pointermove', move);
      window.addEventListener('pointerup', stop, { once: true });
    });
    card.addEventListener('click', event => {
      if (card.dataset.gameDragged === 'true') { delete card.dataset.gameDragged; event.preventDefault(); return; }
      options.selectNode(card.dataset.gameNode || '');
    });
  });
  void viewport;
  void render;
}

function bindLinkInteractions(stage: HTMLElement, viewport: HTMLElement, view: GraphView, options: CanvasOptions) {
  stage.querySelectorAll<HTMLElement>('[data-game-link-source]').forEach(handle => handle.addEventListener('pointerdown', event => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    const sourceId = handle.dataset.gameLinkSource || '';
    const source = (options.game.nodes || []).find(node => node.id === sourceId);
    const svg = stage.querySelector<SVGSVGElement>('svg');
    if (!source || !svg) return;
    const draft = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    draft.classList.add('game-edge-draft');
    svg.append(draft);
    const start = { x: source.position_x + NODE_WIDTH, y: source.position_y + NODE_HEIGHT / 2 };
    const draw = (clientX: number, clientY: number) => {
      const end = worldPoint(pointInViewport(clientX, clientY, viewport), view);
      draft.setAttribute('d', draftPath(start, end));
    };
    draw(event.clientX, event.clientY);
    const move = (next: PointerEvent) => draw(next.clientX, next.clientY);
    const stop = (next: PointerEvent) => {
      window.removeEventListener('pointermove', move);
      draft.remove();
      const target = document.elementFromPoint(next.clientX, next.clientY)?.closest<HTMLElement>('[data-game-node]');
      const targetId = target?.dataset.gameNode;
      if (targetId && targetId !== sourceId) options.createEdge(sourceId, targetId);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', stop, { once: true });
  }));
}

async function persistNodePosition(options: CanvasOptions, node: GameNode) {
  try {
    const response = await fetch(`${options.apiBaseUrl}/games/${options.game.id}/nodes/${node.id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ position_x: node.position_x, position_y: node.position_y }) });
    if (!response.ok) throw new Error();
  } catch {
    options.toast('节点位置保存失败，已恢复图谱');
    await options.reload();
  }
}

function updateEdges(stage: HTMLElement) {
  const origin = graphOrigin(stage);
  const positions = new Map<string, GameNode>();
  stage.querySelectorAll<HTMLElement>('[data-game-node]').forEach(card => positions.set(card.dataset.gameNode || '', { id: '', node_type: '', title: '', original_text: '', prompt: '', duration_seconds: 0, status: '', position_x: Number.parseFloat(card.style.left) - origin, position_y: Number.parseFloat(card.style.top) - origin }));
  stage.querySelectorAll<SVGPathElement>('.game-edge-line').forEach(line => {
    const source = positions.get(line.dataset.sourceNode || '');
    const target = positions.get(line.dataset.targetNode || '');
    if (!source || !target) return;
    line.setAttribute('d', edgeShape(source, target, Number(line.dataset.edgeOffset || 0)).d);
  });
  stage.querySelectorAll<HTMLElement>('.game-edge-label').forEach(label => {
    const edge = label.dataset.gameEdge;
    const line = edge ? stage.querySelector<SVGPathElement>(`.game-edge-line[data-game-edge="${cssEscape(edge)}"]`) : null;
    const source = line && positions.get(line.dataset.sourceNode || '');
    const target = line && positions.get(line.dataset.targetNode || '');
    if (!source || !target || !line) return;
    const shape = edgeShape(source, target, Number(line.dataset.edgeOffset || 0));
    label.style.left = `${shape.label.x + origin}px`;
    label.style.top = `${shape.label.y + origin}px`;
  });
}

function edgeOffsets(edges: GameEdge[]) {
  const grouped = new Map<string, GameEdge[]>();
  for (const edge of edges) {
    const key = `${edge.source_node_id}\u0000${edge.target_node_id}`;
    grouped.set(key, [...(grouped.get(key) || []), edge]);
  }
  const values = new Map<string, number>();
  for (const group of grouped.values()) group.forEach((edge, index) => values.set(edge.id, (index - (group.length - 1) / 2) * 30));
  return values;
}

function edgeShape(source: GameNode, target: GameNode, offset: number) {
  const start = { x: source.position_x + NODE_WIDTH, y: source.position_y + NODE_HEIGHT / 2 };
  const end = { x: target.position_x, y: target.position_y + NODE_HEIGHT / 2 };
  const span = Math.max(55, Math.abs(end.x - start.x) * 0.45);
  const label = { x: (start.x + 3 * (start.x + span) + 3 * (end.x - span) + end.x) / 8, y: (start.y + 3 * (start.y + offset) + 3 * (end.y + offset) + end.y) / 8 };
  return { d: `M ${start.x} ${start.y} C ${start.x + span} ${start.y + offset}, ${end.x - span} ${end.y + offset}, ${end.x} ${end.y}`, label, offset };
}

function draftPath(start: Point, end: Point) {
  const span = Math.max(55, Math.abs(end.x - start.x) * 0.45);
  return `M ${start.x} ${start.y} C ${start.x + span} ${start.y}, ${end.x - span} ${end.y}, ${end.x} ${end.y}`;
}

function pointInViewport(clientX: number, clientY: number, viewport: HTMLElement): Point { const rect = viewport.getBoundingClientRect(); return { x: clientX - rect.left, y: clientY - rect.top }; }
function worldPoint(point: Point, view: GraphView): Point { return { x: (point.x - view.panX) / view.scale, y: (point.y - view.panY) / view.scale }; }
function zoomAround(viewport: HTMLElement, view: GraphView, multiplier: number, render: () => void) { const center = { x: viewport.clientWidth / 2, y: viewport.clientHeight / 2 }; const before = worldPoint(center, view); view.scale = clamp(view.scale * multiplier, MIN_SCALE, MAX_SCALE); view.panX = center.x - before.x * view.scale; view.panY = center.y - before.y * view.scale; render(); }
function nodeBounds(nodes: GameNode[]) { const minX = Math.min(...nodes.map(node => node.position_x)); const minY = Math.min(...nodes.map(node => node.position_y)); const maxX = Math.max(...nodes.map(node => node.position_x + NODE_WIDTH)); const maxY = Math.max(...nodes.map(node => node.position_y + NODE_HEIGHT)); return { x: minX, y: minY, width: maxX - minX, height: maxY - minY }; }
function nodeTypeLabel(type: string) { return type === 'start' ? '起点' : type === 'success' ? '成功' : type === 'failure' ? '失败' : '节点'; }
function clamp(value: number, min: number, max: number) { return Math.max(min, Math.min(max, value)); }
function graphOrigin(stage: HTMLElement) { return Number(stage.dataset.gameGraphOrigin || 0); }
function cssEscape(value: string) { return value.replace(/(["\\])/g, '\\$1'); }
