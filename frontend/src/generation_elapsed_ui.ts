/** Wall-clock stopwatch shared by the game and short-drama generation banners. */

export const MODEL_GENERATION_WAIT_NOTICE = '调用大模型生产时间可能较长';
export const MODEL_GENERATION_EXIT_WARNING = '请勿退出应用';

const TIMER_INTERVAL_MS = 1_000;
const HAS_TIMEZONE = /[zZ]|[+-]\d{2}:?\d{2}$/;

/** Parse the durable task timestamp, falling back to now for legacy rows. */
export function generationStartedAtMs(startedAt: string | null | undefined, now = Date.now()) {
  const parsed = parseGenerationTimestampMs(startedAt);
  return Number.isFinite(parsed) ? Math.min(parsed, now) : now;
}

/** Backend timestamps are UTC; timezone-less values must not be read as local wall-clock. */
export function parseGenerationTimestampMs(startedAt: string | null | undefined) {
  const raw = (startedAt || '').trim();
  if (!raw) return Number.NaN;
  const parsed = Date.parse(normalizeGenerationTimestamp(raw));
  return Number.isFinite(parsed) ? parsed : Number.NaN;
}

function normalizeGenerationTimestamp(value: string) {
  let text = value.replace(' ', 'T').replace(/(\.\d{3})\d+/, '$1');
  if (/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}/.test(text) && !HAS_TIMEZONE.test(text)) text += 'Z';
  return text;
}

/** Calculate total wall-clock time, including page changes and application downtime. */
export function generationElapsedMs(startedAt: string | null | undefined, now = Date.now()) {
  return Math.max(0, now - generationStartedAtMs(startedAt, now));
}

/** Always show all requested units, including zero-valued hours and minutes. */
export function formatGenerationElapsed(elapsedMs: number) {
  const totalSeconds = Math.max(0, Math.floor(elapsedMs / 1_000));
  const hours = Math.floor(totalSeconds / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  const seconds = totalSeconds % 60;
  return `${hours}小时${minutes}分钟${seconds}秒`;
}

export function generationElapsedNotice(elapsedMs: number) {
  return `${generationElapsedLead(elapsedMs)}，${MODEL_GENERATION_EXIT_WARNING}`;
}

function generationElapsedLead(elapsedMs: number) {
  return `已经花费${formatGenerationElapsed(elapsedMs)}，${MODEL_GENERATION_WAIT_NOTICE}`;
}

/** Avoid notifying broad MutationObservers when a timer label has not changed. */
export function setGenerationTextIfChanged(
  element: { textContent: string | null },
  value: string,
) {
  if (element.textContent === value) return false;
  element.textContent = value;
  return true;
}

export function generationElapsedTitleMarkup(
  baseTitle: string,
  key: string,
  startedAt: string | null | undefined,
  escapeHtml: (value: unknown) => string,
) {
  const persistedStart = startedAt || new Date().toISOString();
  const lead = generationElapsedLead(generationElapsedMs(persistedStart));
  return `<span data-generation-title-base>${escapeHtml(baseTitle)}</span>（<span data-generation-elapsed-key="${escapeHtml(key)}" data-generation-started-at="${escapeHtml(persistedStart)}"><span data-generation-elapsed-text>${escapeHtml(lead)}</span>，<span class="generation-exit-warning" data-generation-exit-warning>${escapeHtml(MODEL_GENERATION_EXIT_WARNING)}</span></span>）`;
}

function ensureGenerationElapsedContent(elapsed: HTMLElement) {
  let text = elapsed.querySelector<HTMLElement>('[data-generation-elapsed-text]');
  let warning = elapsed.querySelector<HTMLElement>('[data-generation-exit-warning]');
  if (!text || !warning) {
    elapsed.replaceChildren();
    text = document.createElement('span');
    text.dataset.generationElapsedText = 'true';
    warning = document.createElement('span');
    warning.className = 'generation-exit-warning';
    warning.dataset.generationExitWarning = 'true';
    elapsed.append(text, document.createTextNode('，'), warning);
  }
  setGenerationTextIfChanged(warning, MODEL_GENERATION_EXIT_WARNING);
  return text;
}

/** Preserve the stopwatch span while task polling changes the current generation step. */
export function syncGenerationElapsedTitle(
  title: HTMLElement,
  baseTitle: string,
  key: string,
  startedAt: string | null | undefined,
) {
  let base = title.querySelector<HTMLElement>('[data-generation-title-base]');
  let elapsed = title.querySelector<HTMLElement>('[data-generation-elapsed-key]');
  if (!base || !elapsed || elapsed.dataset.generationElapsedKey !== key) {
    title.replaceChildren();
    base = document.createElement('span');
    base.dataset.generationTitleBase = 'true';
    elapsed = document.createElement('span');
    title.append(base, document.createTextNode('（'), elapsed, document.createTextNode('）'));
  }
  const persistedStart = startedAt || elapsed.dataset.generationStartedAt || new Date().toISOString();
  setGenerationTextIfChanged(base, baseTitle);
  elapsed.dataset.generationElapsedKey = key;
  elapsed.dataset.generationStartedAt = persistedStart;
  setGenerationTextIfChanged(
    ensureGenerationElapsedContent(elapsed),
    generationElapsedLead(generationElapsedMs(persistedStart)),
  );
}

/** Refresh visible labels from durable start times; elapsed time does not depend on these ticks. */
export function tickGenerationElapsedTimers(now = Date.now()) {
  if (typeof document === 'undefined') return;
  document.querySelectorAll<HTMLElement>('[data-generation-elapsed-key]').forEach(element => {
    setGenerationTextIfChanged(
      ensureGenerationElapsedContent(element),
      generationElapsedLead(generationElapsedMs(element.dataset.generationStartedAt, now)),
    );
  });
}

if (typeof window !== 'undefined') {
  window.setInterval(() => tickGenerationElapsedTimers(), TIMER_INTERVAL_MS);
}
