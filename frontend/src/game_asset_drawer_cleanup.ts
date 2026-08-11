/** Removes the detached game material drawer when its editor is no longer active. */
export function closeGameAssetDrawer(root: { querySelector(selector: string): { remove(): void } | null } = document) {
  root.querySelector('[data-game-material-sheet]')?.remove();
}

/** Remove every detached game-editor panel before the route leaves its owning workbench. */
export function closeGameEditorPanels(root: { querySelector(selector: string): { remove(): void } | null } = document) {
  closeGameAssetDrawer(root);
  root.querySelector('[data-game-cover-backdrop]')?.remove();
  root.querySelector('.game-cover-picker-backdrop')?.remove();
  root.querySelector('.game-placeholder-backdrop')?.remove();
}
