/** Fresh-restart action for cancelled project cards, kept outside the card navigation handler. */

import { apiBaseUrl } from './desktop_api.js';

document.addEventListener('click', event => {
  const target = event.target instanceof HTMLElement ? event.target : null;
  const button = target?.closest<HTMLButtonElement>('[data-restart-project]');
  if (!button?.dataset.restartProject) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  const projectId = button.dataset.restartProject;
  button.disabled = true;
  button.textContent = '重新启动中…';
  void fetch(`${apiBaseUrl()}/projects/${projectId}/script-decomposition/restart`, { method: 'POST' })
    .then(async response => { if (!response.ok) throw new Error(`HTTP ${response.status}`); })
    .then(() => window.dispatchEvent(new Event('drama-project-restarted')))
    .catch(error => { console.error(error); button.disabled = false; button.textContent = '重新开始'; window.dispatchEvent(new Event('drama-project-restart-failed')); });
}, true);
