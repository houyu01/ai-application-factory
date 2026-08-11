import assert from 'node:assert/strict';
import test from 'node:test';

import { restoreGameEditorScroll } from '../src/game_scroll_restore.ts';

test('game workbench refresh restores the current main-pane scroll position', () => {
  const pane = { scrollTop: 0 };

  restoreGameEditorScroll(684, pane);

  assert.equal(pane.scrollTop, 684);
});

test('game workbench refresh skips a missing main pane', () => {
  assert.doesNotThrow(() => restoreGameEditorScroll(684, null));
});
