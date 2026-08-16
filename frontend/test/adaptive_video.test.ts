import assert from 'node:assert/strict';
import test from 'node:test';

import { videoPresentation } from '../src/adaptive_video.ts';

test('adaptive video preserves a landscape source ratio', () => {
  assert.deepEqual(videoPresentation(1920, 1080), {
    aspectRatio: '1920 / 1080',
    height: 1080,
    orientation: 'landscape',
    width: 1920,
  });
});

test('adaptive video recognizes portrait and square sources', () => {
  assert.equal(videoPresentation(1080, 1920).orientation, 'portrait');
  assert.equal(videoPresentation(1080, 1080).orientation, 'square');
});

test('adaptive video uses a stable ratio before metadata is available', () => {
  assert.deepEqual(videoPresentation(0, Number.NaN), {
    aspectRatio: '16 / 9',
    height: 9,
    orientation: 'unknown',
    width: 16,
  });
});
