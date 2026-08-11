import assert from 'node:assert/strict';
import test from 'node:test';

import { gameCanvasWidthFromPointer } from '../src/game_canvas_resize.ts';

test('canvas resize reserves the rail, 3px divider, and minimum inspector width', () => {
  assert.equal(gameCanvasWidthFromPointer(100, 0, 1500), 360);
  assert.equal(gameCanvasWidthFromPointer(900, 0, 1500), 813);
  assert.equal(gameCanvasWidthFromPointer(2000, 0, 1500), 1080);
});
