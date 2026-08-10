import { restoreSettingsScroll } from './settings_ui.js';

function mainPane() {
  return document.querySelector<HTMLElement>('.shell > main');
}

/** Restore the settings pane after a select change triggers the legacy full-page renderer. */
document.addEventListener('change', event => {
  const select = event.target instanceof HTMLSelectElement ? event.target : null;
  if (!select?.closest('.settings-grid')) return;
  const scrollTop = mainPane()?.scrollTop;
  requestAnimationFrame(() => restoreSettingsScroll(true, scrollTop, mainPane()));
}, true);
