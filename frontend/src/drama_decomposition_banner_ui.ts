/**
 * Project-level status banner for the initial script decomposition workflow.
 *
 * The detail toolbar observer calls this after a newly-created short drama is
 * rendered. It gives creators persistent feedback while the durable
 * `script_decomposition` task is extracting shots and reusable assets. The
 * banner is removed as soon as that task no longer runs, including after a
 * recovered task finishes following a backend restart.
 */
import type { ApiProject } from './models.js';

function isScriptDecompositionRunning(project: ApiProject | null): boolean {
  return Boolean(project?.tasks?.some(task => (
    task.type === 'script_decomposition' && task.status === '生成中'
  )));
}

/** Synchronize the detail-page banner with the persisted initial task status. */
export function syncDramaDecompositionBanner(project: ApiProject | null) {
  const detail = document.querySelector<HTMLElement>('.drama-detail');
  if (!detail) return;
  const current = detail.querySelector<HTMLElement>('[data-drama-decomposition-banner]');
  if (!isScriptDecompositionRunning(project)) {
    current?.remove();
    return;
  }
  if (current) return;
  const toolbar = detail.querySelector<HTMLElement>('.drama-detail-toolbar');
  if (!toolbar) return;
  const banner = document.createElement('section');
  banner.className = 'drama-decomposition-banner';
  banner.dataset.dramaDecompositionBanner = 'true';
  banner.setAttribute('role', 'status');
  banner.innerHTML = '<span class="generation-spinner" aria-hidden="true"></span><div><span class="drama-decomposition-banner-title">剧本正在后台提取</span><p>正在拆解分镜、角色、场景和道具；完成后会自动显示编辑内容。</p></div>';
  toolbar.insertAdjacentElement('afterend', banner);
}
