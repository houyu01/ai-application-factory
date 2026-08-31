/** Resolve a node video to a wx.createVideo src: CDN URL or a USER_DATA_PATH copy of packaged media/. */

const copied = {};

function remoteSrc(graph, relPath) {
  const base = String(graph.videoBaseUrl || '').replace(/\/$/, '');
  return base ? `${base}/${relPath}` : '';
}

function localDest(relPath) {
  return `${wx.env.USER_DATA_PATH}/${relPath.replace(/\//g, '_')}`;
}

function copyPackagedVideo(relPath) {
  if (copied[relPath]) return copied[relPath];
  const dest = localDest(relPath);
  const fs = wx.getFileSystemManager();
  try {
    fs.accessSync(dest);
    copied[relPath] = dest;
    return dest;
  } catch (_) {
    // Destination is missing; copy from the minigame package next.
  }
  try {
    fs.copyFileSync(relPath, dest);
    copied[relPath] = dest;
    return dest;
  } catch (copyError) {
    try {
      fs.writeFileSync(dest, fs.readFileSync(relPath));
      copied[relPath] = dest;
      return dest;
    } catch (writeError) {
      console.warn('minigame video copy failed', relPath, copyError, writeError);
      return '';
    }
  }
}

function resolveVideoSrc(graph, relPath) {
  if (!relPath) return '';
  if (/^https?:\/\//.test(relPath)) return relPath;
  return remoteSrc(graph, relPath) || copyPackagedVideo(relPath);
}

module.exports = { resolveVideoSrc };
