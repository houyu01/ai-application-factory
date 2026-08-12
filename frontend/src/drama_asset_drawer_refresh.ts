import { restoreDramaAssetDrawerScroll } from './drama_asset_drawer_scroll.js';

/** Rebuild the open material drawer without discarding the reader's position in its asset list. */
export function replaceDramaAssetDrawer(
  backdrop: HTMLElement,
  markup: string,
  bind: () => void,
  afterBind?: () => void,
) {
  const scrollTop = backdrop.querySelector<HTMLElement>('.drama-asset-sheet')?.scrollTop;
  const wrapper = document.createElement('div');
  wrapper.innerHTML = markup;
  const next = wrapper.firstElementChild as HTMLElement;
  backdrop.replaceWith(next);
  bind();
  afterBind?.();
  const restore = () => restoreDramaAssetDrawerScroll(scrollTop, next.querySelector<HTMLElement>('.drama-asset-sheet'));
  restore();
  requestAnimationFrame(() => { if (next.isConnected) restore(); });
}
