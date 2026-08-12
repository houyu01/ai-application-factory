import assert from 'node:assert/strict';
import test from 'node:test';

import { restoreDramaAssetDrawerScroll } from '../src/drama_asset_drawer_scroll.ts';

test('drama asset drawer keeps its position while image generation refreshes it', () => {
  const drawer = { scrollTop: 0 };

  restoreDramaAssetDrawerScroll(876, drawer);

  assert.equal(drawer.scrollTop, 876);
});
