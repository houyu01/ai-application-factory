/** Restore a short-drama material drawer after generation updates replace its DOM node. */
export function restoreDramaAssetDrawerScroll(scrollTop: number | undefined, target?: { scrollTop: number } | null) {
  if (scrollTop !== undefined && target) target.scrollTop = scrollTop;
}
