import assert from 'node:assert/strict';
import test from 'node:test';

import { gameCanvasWidthFromPointer } from '../src/game_canvas_resize.ts';

test('canvas resize reserves the compact rail and tablet-friendly inspector', () => {
  assert.equal(gameCanvasWidthFromPointer(100, 0, 1500), 180);
  assert.equal(gameCanvasWidthFromPointer(900, 0, 1500), 847);
  assert.equal(gameCanvasWidthFromPointer(2000, 0, 1500), 1194);
});
