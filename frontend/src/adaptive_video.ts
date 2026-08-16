/** Keep every user-controlled video complete while leaving playback controls to WebKit. */

export type VideoOrientation = 'landscape' | 'portrait' | 'square' | 'unknown';

export type VideoPresentation = {
  aspectRatio: string;
  height: number;
  orientation: VideoOrientation;
  width: number;
};

const VIDEO_SELECTOR = 'video[controls]';
let installed = false;

/** Convert intrinsic media dimensions into the CSS ratio and layout mode used by the player. */
export function videoPresentation(width: number, height: number): VideoPresentation {
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    return { aspectRatio: '16 / 9', height: 9, orientation: 'unknown', width: 16 };
  }
  const normalizedWidth = Math.round(width);
  const normalizedHeight = Math.round(height);
  const orientation = normalizedWidth > normalizedHeight
    ? 'landscape'
    : normalizedWidth < normalizedHeight ? 'portrait' : 'square';
  return {
    aspectRatio: `${normalizedWidth} / ${normalizedHeight}`,
    height: normalizedHeight,
    orientation,
    width: normalizedWidth,
  };
}

function syncVideoDimensions(video: HTMLVideoElement, shell: HTMLElement) {
  const presentation = videoPresentation(video.videoWidth, video.videoHeight);
  shell.style.setProperty('--adaptive-video-aspect', presentation.aspectRatio);
  shell.dataset.videoOrientation = presentation.orientation;
  shell.dataset.videoWidth = String(presentation.width);
  shell.dataset.videoHeight = String(presentation.height);
}

function createShell(video: HTMLVideoElement) {
  const gameShell = video.parentElement?.classList.contains('game-player-video-wrap')
    ? video.parentElement
    : null;
  if (gameShell) return gameShell;
  const shell = document.createElement('div');
  video.before(shell);
  shell.append(video);
  return shell;
}

function enhanceVideo(video: HTMLVideoElement) {
  if (video.dataset.adaptiveVideoReady === 'true') return;
  video.dataset.adaptiveVideoReady = 'true';
  const shell = createShell(video);
  shell.classList.add('adaptive-video-shell');
  video.addEventListener('loadedmetadata', () => syncVideoDimensions(video, shell));
  video.addEventListener('emptied', () => syncVideoDimensions(video, shell));
  syncVideoDimensions(video, shell);
}

function scanVideos(root: ParentNode) {
  if (root instanceof HTMLVideoElement && root.matches(VIDEO_SELECTOR)) enhanceVideo(root);
  root.querySelectorAll<HTMLVideoElement>(VIDEO_SELECTOR).forEach(enhanceVideo);
}

/** Observe route and modal rendering so dynamically inserted playable videos get the same behavior. */
export function installAdaptiveVideoPlayers(root: ParentNode = document) {
  if (installed) return;
  installed = true;
  scanVideos(root);
  const observer = new MutationObserver(mutations => {
    for (const mutation of mutations) {
      if (mutation.type === 'attributes' && mutation.target instanceof HTMLVideoElement) {
        if (mutation.target.matches(VIDEO_SELECTOR)) enhanceVideo(mutation.target);
        continue;
      }
      mutation.addedNodes.forEach(node => { if (node instanceof Element) scanVideos(node); });
    }
  });
  const observedRoot = root instanceof Document ? root.documentElement : root;
  observer.observe(observedRoot, { attributeFilter: ['controls'], attributes: true, childList: true, subtree: true });
}
