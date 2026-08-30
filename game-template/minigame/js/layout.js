/** Portrait layout for the native video layer plus canvas choice panel. */

function metrics() {
  const info = wx.getSystemInfoSync();
  const width = info.screenWidth;
  const height = info.screenHeight;
  const pad = 16;
  const topbar = 56;
  const videoWidth = width - pad * 2;
  const videoHeight = Math.round(videoWidth * 9 / 16);
  const caption = 44;
  const video = { x: pad, y: topbar, width: videoWidth, height: videoHeight };
  const restart = { x: width - pad - 88, y: 12, width: 88, height: 32 };
  return {
    width,
    height,
    pixelRatio: info.pixelRatio || 1,
    pad,
    topbar,
    video,
    caption,
    restart,
    choicesTop: video.y + video.height + caption,
  };
}

function choiceRects(count, layout) {
  const gap = 10;
  const available = layout.height - layout.choicesTop - layout.pad;
  const height = Math.min(56, Math.max(44, (available - gap * Math.max(count - 1, 0)) / Math.max(count, 1)));
  const rects = [];
  for (let index = 0; index < count; index += 1) {
    rects.push({
      x: layout.pad,
      y: layout.choicesTop + index * (height + gap),
      width: layout.width - layout.pad * 2,
      height,
    });
  }
  return rects;
}

function hit(point, rect) {
  return point.x >= rect.x && point.x <= rect.x + rect.width && point.y >= rect.y && point.y <= rect.y + rect.height;
}

module.exports = { metrics, choiceRects, hit };
