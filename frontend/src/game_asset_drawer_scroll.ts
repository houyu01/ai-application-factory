/** Preserve the material drawer position while its generation state is re-rendered. */
export function restoreGameAssetDrawerScroll(scrollTop: number | undefined, target?: { scrollTop: number } | null) {
  if (scrollTop !== undefined && target) target.scrollTop = scrollTop;
}
