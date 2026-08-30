/** Canvas chrome for the minigame player: top bar, captions, choices, endings. */

const COLORS = {
  bg: '#0d0f12',
  panel: '#20252c',
  border: '#3a424c',
  text: '#f4f6f8',
  muted: '#9aa4b1',
  accent: '#e19b58',
  success: '#82d6a2',
  failure: '#e88678',
};

function roundRect(ctx, x, y, width, height, radius) {
  const r = Math.min(radius, width / 2, height / 2);
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + width, y, x + width, y + height, r);
  ctx.arcTo(x + width, y + height, x, y + height, r);
  ctx.arcTo(x, y + height, x, y, r);
  ctx.arcTo(x, y, x + width, y, r);
  ctx.closePath();
}

function fillRound(ctx, rect, fill, stroke) {
  roundRect(ctx, rect.x, rect.y, rect.width, rect.height, 12);
  ctx.fillStyle = fill;
  ctx.fill();
  if (stroke) {
    ctx.strokeStyle = stroke;
    ctx.lineWidth = 1;
    ctx.stroke();
  }
}

function drawTopbar(ctx, layout, gameName) {
  ctx.fillStyle = COLORS.muted;
  ctx.font = '11px sans-serif';
  ctx.fillText('INTERACTIVE VIDEO GAME', layout.pad, 22);
  ctx.fillStyle = COLORS.text;
  ctx.font = 'bold 18px sans-serif';
  ctx.fillText(gameName, layout.pad, 44);
  fillRound(ctx, layout.restart, COLORS.panel, COLORS.border);
  ctx.fillStyle = COLORS.text;
  ctx.font = '13px sans-serif';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillText('重新开始', layout.restart.x + layout.restart.width / 2, layout.restart.y + layout.restart.height / 2);
  ctx.textAlign = 'left';
  ctx.textBaseline = 'alphabetic';
}

function drawCaption(ctx, layout, node, ended, waiting) {
  const y = layout.video.y + layout.video.height + 28;
  ctx.fillStyle = COLORS.text;
  ctx.font = 'bold 16px sans-serif';
  ctx.fillText(node.title, layout.pad, y);
  ctx.fillStyle = COLORS.muted;
  ctx.font = '12px sans-serif';
  const status = ended
    ? (node.type === 'success' ? '成功结局' : '失败结局')
    : waiting
      ? '点击选项继续'
      : (node.video ? '视频播放中' : '待补充视频');
  ctx.textAlign = 'right';
  ctx.fillText(status, layout.width - layout.pad, y);
  ctx.textAlign = 'left';
}

function drawChoices(ctx, edges, rects) {
  edges.forEach((edge, index) => {
    const rect = rects[index];
    fillRound(ctx, rect, COLORS.panel, COLORS.border);
    const badge = { x: rect.x + 12, y: rect.y + (rect.height - 26) / 2, width: 26, height: 26 };
    roundRect(ctx, badge.x, badge.y, badge.width, badge.height, 13);
    ctx.fillStyle = '#343b45';
    ctx.fill();
    ctx.fillStyle = COLORS.accent;
    ctx.font = 'bold 13px sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(String.fromCharCode(65 + index), badge.x + 13, badge.y + 13);
    ctx.fillStyle = COLORS.text;
    ctx.font = '15px sans-serif';
    ctx.textAlign = 'left';
    ctx.fillText(edge.text, badge.x + 36, rect.y + rect.height / 2);
    ctx.textBaseline = 'alphabetic';
  });
}

function drawEnding(ctx, layout, node) {
  const success = node.type === 'success';
  ctx.textAlign = 'center';
  ctx.fillStyle = success ? COLORS.success : COLORS.failure;
  ctx.font = '36px sans-serif';
  ctx.fillText(success ? '✦' : '○', layout.width / 2, layout.choicesTop + 36);
  ctx.fillStyle = COLORS.text;
  ctx.font = 'bold 20px sans-serif';
  ctx.fillText(success ? '成功结局' : '失败结局', layout.width / 2, layout.choicesTop + 72);
  ctx.fillStyle = COLORS.muted;
  ctx.font = '14px sans-serif';
  wrapText(ctx, node.text, layout.width / 2, layout.choicesTop + 100, layout.width - layout.pad * 2, 22);
  const button = endingButton(layout);
  fillRound(ctx, button, COLORS.accent, '');
  ctx.fillStyle = '#17191c';
  ctx.font = 'bold 15px sans-serif';
  ctx.textBaseline = 'middle';
  ctx.fillText('再玩一次', button.x + button.width / 2, button.y + button.height / 2);
  ctx.textAlign = 'left';
  ctx.textBaseline = 'alphabetic';
}

function endingButton(layout) {
  return { x: layout.width / 2 - 70, y: layout.height - layout.pad - 48, width: 140, height: 40 };
}

function wrapText(ctx, text, x, y, maxWidth, lineHeight) {
  const chars = Array.from(text);
  let line = '';
  let cursor = y;
  chars.forEach((char) => {
    const next = line + char;
    if (ctx.measureText(next).width > maxWidth && line) {
      ctx.fillText(line, x, cursor);
      line = char;
      cursor += lineHeight;
    } else {
      line = next;
    }
  });
  if (line) ctx.fillText(line, x, cursor);
}

function drawScene(ctx, layout, game, node, edges, rects, ended, waiting) {
  ctx.fillStyle = COLORS.bg;
  ctx.fillRect(0, 0, layout.width, layout.height);
  drawTopbar(ctx, layout, game.name);
  ctx.fillStyle = '#060708';
  ctx.fillRect(layout.video.x, layout.video.y, layout.video.width, layout.video.height);
  if (!node.video) {
    ctx.fillStyle = COLORS.muted;
    ctx.textAlign = 'center';
    ctx.fillText('当前节点尚未配置视频', layout.width / 2, layout.video.y + layout.video.height / 2);
    ctx.textAlign = 'left';
  }
  drawCaption(ctx, layout, node, ended, waiting);
  if (ended) drawEnding(ctx, layout, node);
  else if (waiting) drawChoices(ctx, edges, rects);
}

module.exports = { drawScene, endingButton };
