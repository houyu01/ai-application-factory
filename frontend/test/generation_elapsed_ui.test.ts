import assert from 'node:assert/strict';
import test from 'node:test';

import {
  formatGenerationElapsed,
  generationElapsedMs,
  generationElapsedNotice,
  setGenerationTextIfChanged,
} from '../src/generation_elapsed_ui.ts';

test('generation elapsed time always renders hours, minutes, and seconds', () => {
  assert.equal(formatGenerationElapsed(0), '0小时0分钟0秒');
  assert.equal(formatGenerationElapsed(3_661_999), '1小时1分钟1秒');
  assert.equal(
    generationElapsedNotice(3_661_999),
    '已经花费1小时1分钟1秒，调用大模型生产时间可能较长，请勿退出应用',
  );
});

test('generation stopwatch includes page changes and application downtime', () => {
  const now = Date.parse('2026-08-14T11:01:01.000Z');
  assert.equal(generationElapsedMs('2026-08-14T10:00:00.000Z', now), 3_661_000);
});

test('legacy tasks without a timestamp restart from the current instant', () => {
  assert.equal(generationElapsedMs(undefined, 12_345), 0);
});

test('unchanged timer text does not mutate the DOM and retrigger observers', () => {
  let writes = 0;
  let text = '已经花费0小时0分钟1秒';
  const element = {
    get textContent() { return text; },
    set textContent(value: string | null) { writes += 1; text = value || ''; },
  };

  assert.equal(setGenerationTextIfChanged(element, text), false);
  assert.equal(writes, 0);
  assert.equal(setGenerationTextIfChanged(element, '已经花费0小时0分钟2秒'), true);
  assert.equal(writes, 1);
});
