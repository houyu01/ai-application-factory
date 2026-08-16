type ScrollTarget = { scrollTop: number; scrollLeft: number };

export type DramaScrollTargets = {
  main?: ScrollTarget | null;
  episodeList?: ScrollTarget | null;
  videoHistory?: ScrollTarget | null;
  videoHistoryKey?: string | null;
};

type ScrollPosition = { top: number; left: number };

export type DramaScrollState = {
  main?: ScrollPosition;
  episodeList?: ScrollPosition;
  videoHistory?: ScrollPosition & { key: string | null };
};

function readPosition(target?: ScrollTarget | null): ScrollPosition | undefined {
  return target ? { top: target.scrollTop, left: target.scrollLeft } : undefined;
}

function restorePosition(position: ScrollPosition | undefined, target?: ScrollTarget | null) {
  if (!position || !target) return;
  target.scrollTop = position.top;
  target.scrollLeft = position.left;
}

export function captureDramaScrollState(targets: DramaScrollTargets): DramaScrollState {
  const videoHistory = readPosition(targets.videoHistory);
  return {
    main: readPosition(targets.main),
    episodeList: readPosition(targets.episodeList),
    videoHistory: videoHistory ? { ...videoHistory, key: targets.videoHistoryKey || null } : undefined,
  };
}

export function restoreDramaScrollState(state: DramaScrollState, targets: DramaScrollTargets) {
  restorePosition(state.main, targets.main);
  restorePosition(state.episodeList, targets.episodeList);
  if (state.videoHistory?.key === (targets.videoHistoryKey || null)) restorePosition(state.videoHistory, targets.videoHistory);
}

function currentTargets(root: ParentNode = document): DramaScrollTargets {
  const videoHistory = root.querySelector<HTMLElement>('.drama-history-scroll, .drama-history-grid');
  return {
    main: root.querySelector<HTMLElement>('.shell > main'),
    episodeList: root.querySelector<HTMLElement>('.drama-episode-list'),
    videoHistory,
    videoHistoryKey: videoHistory?.closest<HTMLElement>('[data-drama-history-shot-id]')?.dataset.dramaHistoryShotId || null,
  };
}

export function captureCurrentDramaScrollState(enabled: boolean): DramaScrollState | null {
  return enabled ? captureDramaScrollState(currentTargets()) : null;
}

export function scheduleDramaScrollRestore(state: DramaScrollState | null) {
  if (!state) return;
  requestAnimationFrame(() => restoreDramaScrollState(state, currentTargets()));
}
