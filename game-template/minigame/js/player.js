/**
 * Runtime for the packaged graph: play each node with wx.createVideo, then accept a choice.
 * Canvas draws chrome and options; the native video layer occupies the reserved 16:9 slot.
 */
const graph = require('./graph');
const { metrics, choiceRects, hit } = require('./layout');
const { resolveVideoSrc } = require('./media');
const { drawScene, endingButton } = require('./draw');

function nodeById(id) {
  return graph.nodes.find((item) => item.id === id);
}

function edgesFrom(id) {
  return graph.edges.filter((edge) => edge.from === id);
}

function isEnding(node) {
  return node.type === 'success' || node.type === 'failure';
}

function startPlayer() {
  const layout = metrics();
  const canvas = wx.createCanvas();
  canvas.width = Math.round(layout.width * layout.pixelRatio);
  canvas.height = Math.round(layout.height * layout.pixelRatio);
  const ctx = canvas.getContext('2d');
  ctx.scale(layout.pixelRatio, layout.pixelRatio);

  const state = { nodeId: 'start', waiting: false };
  let video = null;
  let rects = [];

  function destroyVideo() {
    if (!video) return;
    try { video.stop(); } catch (_) { /* already stopped */ }
    try { video.destroy(); } catch (_) { /* already destroyed */ }
    video = null;
  }

  function revealChoices() {
    state.waiting = true;
    draw();
  }

  function draw() {
    const node = nodeById(state.nodeId);
    const edges = edgesFrom(node.id);
    rects = state.waiting && !isEnding(node) ? choiceRects(edges.length, layout) : [];
    drawScene(ctx, layout, graph, node, edges, rects, isEnding(node) && state.waiting, state.waiting);
  }

  function playCurrent() {
    destroyVideo();
    state.waiting = false;
    const node = nodeById(state.nodeId);
    draw();
    if (!node.video) {
      state.waiting = true;
      draw();
      return;
    }
    const src = resolveVideoSrc(graph, node.video);
    if (!src) {
      state.waiting = true;
      draw();
      return;
    }
    const slot = layout.video;
    video = wx.createVideo({
      x: slot.x,
      y: slot.y,
      width: slot.width,
      height: slot.height,
      src,
      autoplay: true,
      controls: false,
      showCenterPlayBtn: false,
      objectFit: 'cover',
      enableProgressGesture: false,
    });
    video.onEnded(revealChoices);
    video.onError(revealChoices);
  }

  function restart() {
    state.nodeId = 'start';
    playCurrent();
  }

  wx.onTouchEnd((event) => {
    const touch = event.changedTouches && event.changedTouches[0];
    if (!touch) return;
    const point = { x: touch.clientX, y: touch.clientY };
    if (hit(point, layout.restart)) {
      restart();
      return;
    }
    const node = nodeById(state.nodeId);
    if (!state.waiting) return;
    if (isEnding(node)) {
      if (hit(point, endingButton(layout))) restart();
      return;
    }
    const edges = edgesFrom(node.id);
    const index = rects.findIndex((rect) => hit(point, rect));
    if (index < 0) return;
    state.nodeId = edges[index].to;
    playCurrent();
  });

  playCurrent();
}

module.exports = { startPlayer };
