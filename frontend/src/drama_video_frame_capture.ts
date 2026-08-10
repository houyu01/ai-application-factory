/** Decode a playable video frame before the short-drama frame editor paints it. */

export type DramaFrameSide = 'first' | 'last';

const FRAME_LOAD_TIMEOUT_MS = 12_000;

function waitForVideo(
  video: HTMLVideoElement,
  eventName: 'loadeddata' | 'seeked',
  isReady: () => boolean,
  errorMessage: string,
) {
  return new Promise<void>((resolve, reject) => {
    let timeoutId: number | undefined;
    const cleanup = () => {
      video.removeEventListener(eventName, done);
      video.removeEventListener('error', fail);
      if (timeoutId !== undefined) window.clearTimeout(timeoutId);
    };
    const done = () => {
      cleanup();
      resolve();
    };
    const fail = () => {
      cleanup();
      reject(new Error(errorMessage));
    };
    video.addEventListener(eventName, done, { once: true });
    video.addEventListener('error', fail, { once: true });
    timeoutId = window.setTimeout(fail, FRAME_LOAD_TIMEOUT_MS);
    if (isReady()) done();
  });
}

function frameTime(duration: number, side: DramaFrameSide): number {
  const safeDuration = Number.isFinite(duration) && duration > 0 ? duration : 0.1;
  const edgeOffset = Math.min(0.12, Math.max(0.01, safeDuration * 0.02));
  return side === 'last'
    ? Math.max(0, safeDuration - edgeOffset)
    : Math.min(edgeOffset, safeDuration / 2);
}

function nextPaint() {
  return new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
}

/** Extract a decoded near-edge frame, avoiding a black preroll frame at time zero. */
export async function captureDramaVideoFrame(
  url: string,
  side: DramaFrameSide,
  resolveMediaUrl: (value: string) => string,
): Promise<string | null> {
  const source = resolveMediaUrl(url);
  if (!source) return null;
  const video = document.createElement('video');
  video.muted = true;
  video.playsInline = true;
  video.preload = 'auto';
  video.crossOrigin = 'anonymous';
  video.style.cssText = 'position:fixed;left:-1px;top:-1px;width:1px;height:1px;opacity:0;pointer-events:none';
  document.body.append(video);
  try {
    video.src = source;
    video.load();
    await waitForVideo(
      video,
      'loadeddata',
      () => video.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA && video.videoWidth > 0,
      'video frame data unavailable',
    );
    const targetTime = frameTime(video.duration, side);
    if (Math.abs(video.currentTime - targetTime) > 0.001) {
      video.currentTime = targetTime;
      await waitForVideo(video, 'seeked', () => false, 'video frame seek unavailable');
    }
    await nextPaint();
    if (video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA || !video.videoWidth || !video.videoHeight) return null;
    const canvas = document.createElement('canvas');
    canvas.width = video.videoWidth;
    canvas.height = video.videoHeight;
    const context = canvas.getContext('2d');
    if (!context) return null;
    context.drawImage(video, 0, 0, canvas.width, canvas.height);
    return canvas.toDataURL('image/jpeg', 0.88);
  } catch {
    return null;
  } finally {
    video.pause();
    video.removeAttribute('src');
    video.load();
    video.remove();
  }
}
