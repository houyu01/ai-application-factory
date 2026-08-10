/**
 * Per-shot duration and video-count controls for the drama editor.
 *
 * The drama detail page calls this after it has rendered a selected shot. It
 * moves the duration card below the reference images and turns it into a
 * persisted 3–15 second selector. The adjacent count stays in the editor
 * session and is submitted only when the creator generates the shot video.
 */
import type { ApiProject, DramaShot } from './models.js';
import { scheduleDramaEditorAutosave } from './drama_editor_autosave.js';

type DurationRuntime = object;

const DURATION_OPTIONS = Array.from({ length: 13 }, (_, index) => index + 3);
const VIDEO_COUNT_OPTIONS = [1, 2, 3];
const videoCountByShot = new Map<string, number>();

function selectOptions(values: number[], value: number, suffix: string) {
  return values.map(item => (
    `<option value="${item}" ${item === value ? 'selected' : ''}>${item}${suffix}</option>`
  )).join('');
}

function durationField(duration: number, count: number) {
  const durationOptions = selectOptions(DURATION_OPTIONS, duration, 's');
  return `<div class="drama-shot-generation-settings"><div><span>时长</span><select id="drama-shot-duration" aria-label="分镜时长">${durationOptions}</select></div><div><span>一次生成视频数量</span><select id="drama-shot-video-count" aria-label="一次生成视频数量">${selectOptions(VIDEO_COUNT_OPTIONS, count, ' 个')}</select></div></div>`;
}

/** Return the selected parallel-output count for the currently rendered shot. */
export function dramaShotVideoGenerationCount(shotId: string) {
  const select = document.querySelector<HTMLSelectElement>('#drama-shot-video-count');
  const value = select?.dataset.shotId === shotId ? Number(select.value) : videoCountByShot.get(shotId);
  return VIDEO_COUNT_OPTIONS.includes(value || 0) ? value as number : 1;
}

/**
 * Move and activate per-shot duration and video-count controls beneath references.
 *
 * This is kept separate from the main renderer so changing one duration does
 * not require rebuilding the drama workspace or changing project-wide ratio.
 */
export function setupDramaShotDurationControl(
  project: ApiProject,
  shot: DramaShot,
  runtime: DurationRuntime,
) {
  if (!document.querySelector('.drama-detail')) return;
  const parameters = document.querySelector<HTMLElement>('.drama-params');
  if (!parameters || parameters.dataset.durationControlReady === shot.id) return;

  const summaryCards = parameters.querySelectorAll<HTMLElement>(':scope > div');
  const durationCard = summaryCards.item(0);
  if (!durationCard) return;
  durationCard.classList.add('drama-shot-duration-control');
  document.querySelector<HTMLElement>('.drama-reference-panel')?.after(durationCard);
  const count = dramaShotVideoGenerationCount(shot.id);
  durationCard.innerHTML = durationField(
    Math.min(15, Math.max(3, Number(shot.duration_seconds || 10))), count,
  );
  summaryCards.item(1)?.remove();
  parameters.dataset.durationControlReady = shot.id;

  const select = durationCard.querySelector<HTMLSelectElement>('#drama-shot-duration');
  const countSelect = durationCard.querySelector<HTMLSelectElement>('#drama-shot-video-count');
  if (countSelect) {
    countSelect.dataset.shotId = shot.id;
    countSelect.addEventListener('change', () => {
      videoCountByShot.set(shot.id, Number(countSelect.value));
    });
  }
  if (!select) return;
  select.addEventListener('change', scheduleDramaEditorAutosave);
}
