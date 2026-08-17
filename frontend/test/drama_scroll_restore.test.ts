import assert from 'node:assert/strict';
import test from 'node:test';

import { captureDramaScrollState, restoreDramaScrollState } from '../src/drama_scroll_restore.ts';

test('drama rerender restores main, episode-list, and same-shot video-history scroll', () => {
  const state = captureDramaScrollState({
    main: { scrollTop: 420, scrollLeft: 0 },
    episodeList: { scrollTop: 760, scrollLeft: 0 },
    videoHistory: { scrollTop: 0, scrollLeft: 318 },
    videoHistoryKey: 'shot-2',
  });
  const main = { scrollTop: 0, scrollLeft: 0 };
  const episodeList = { scrollTop: 0, scrollLeft: 0 };
  const videoHistory = { scrollTop: 0, scrollLeft: 0 };

  restoreDramaScrollState(state, { main, episodeList, videoHistory, videoHistoryKey: 'shot-2' });

  assert.equal(main.scrollTop, 420);
  assert.equal(episodeList.scrollTop, 760);
  assert.equal(videoHistory.scrollLeft, 318);
});

test('switching shots does not reuse another shot video-history position', () => {
  const state = captureDramaScrollState({
    episodeList: { scrollTop: 760, scrollLeft: 0 },
    videoHistory: { scrollTop: 0, scrollLeft: 318 },
    videoHistoryKey: 'shot-2',
  });
  const episodeList = { scrollTop: 0, scrollLeft: 0 };
  const videoHistory = { scrollTop: 0, scrollLeft: 0 };

  restoreDramaScrollState(state, { episodeList, videoHistory, videoHistoryKey: 'shot-3' });

  assert.equal(episodeList.scrollTop, 760);
  assert.equal(videoHistory.scrollLeft, 0);
});
