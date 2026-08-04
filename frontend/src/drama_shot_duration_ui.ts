/**
 * Per-shot duration control for the drama editor.
 *
 * The drama detail page calls this after it has rendered a selected shot. It
 * replaces the legacy static duration/ratio summary with a persisted 3–15
 * second selector. Ratio intentionally remains a project-level setting.
 */
import type { ApiProject, DramaShot } from './models.js';

type DurationRuntime = {
  apiBaseUrl: string;
  toast: (message: string) => void;
};

const DURATION_OPTIONS = Array.from({ length: 13 }, (_, index) => index + 3);

function durationField(value: number) {
  const options = DURATION_OPTIONS.map(seconds => (
    `<option value="${seconds}" ${seconds === value ? 'selected' : ''}>${seconds}s</option>`
  )).join('');
  return `<span>时长</span><select id="drama-shot-duration" aria-label="分镜时长">${options}</select>`;
}

/**
 * Replace the legacy per-shot parameters with the duration selector.
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
  durationCard.innerHTML = durationField(Math.min(15, Math.max(3, Number(shot.duration_seconds || 10))));
  summaryCards.item(1)?.remove();
  parameters.dataset.durationControlReady = shot.id;

  const select = durationCard.querySelector<HTMLSelectElement>('#drama-shot-duration');
  const generateButton = document.querySelector<HTMLButtonElement>('#drama-generate-shot-video');
  if (!select) return;
  select.addEventListener('change', async () => {
    const durationSeconds = Number(select.value);
    select.disabled = true;
    if (generateButton) generateButton.disabled = true;
    try {
      const response = await fetch(`${runtime.apiBaseUrl}/projects/${project.id}/shots/${shot.id}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ duration_seconds: durationSeconds }),
      });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      shot.duration_seconds = durationSeconds;
      runtime.toast(`分镜时长已设置为 ${durationSeconds}s`);
    } catch (error) {
      select.value = String(shot.duration_seconds || 10);
      runtime.toast('分镜时长保存失败');
      console.error(error);
    } finally {
      select.disabled = false;
      if (generateButton) generateButton.disabled = false;
    }
  });
}
