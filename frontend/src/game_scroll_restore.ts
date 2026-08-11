/** Preserve the game workbench pane after a polling refresh replaces its contents. */
export function restoreGameEditorScroll(scrollTop: number, target?: { scrollTop: number } | null) {
  if (target) target.scrollTop = scrollTop;
}
