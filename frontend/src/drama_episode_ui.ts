/** Episode-list interactions for the drama workbench, including durable in-session collapse state. */
import type { ApiProject } from './models.js';

const collapsedEpisodes = new Set<string>();

function episodeKey(project: ApiProject, index: number) {
  const episode = project.episodes?.[index];
  return `${project.id}:${episode?.id || episode?.title || index}`;
}

function setCollapsed(block: HTMLElement, header: HTMLElement, collapsed: boolean) {
  block.classList.toggle('collapsed', collapsed);
  header.setAttribute('aria-expanded', String(!collapsed));
  header.title = collapsed ? '展开本集分镜' : '收起本集分镜';
}

/**
 * Binds episode collapse controls after the drama detail view is rendered.
 * The workbench calls this whenever project data is refreshed so collapse state
 * survives partial task updates without affecting persisted drama content.
 */
export function bindDramaEpisodeManager(project: ApiProject) {
  document.querySelectorAll<HTMLElement>('.drama-episode-block').forEach((block, index) => {
    const header = block.querySelector<HTMLElement>('.drama-episode-head');
    if (!header || header.dataset.episodeToggleBound === 'true') return;
    const key = episodeKey(project, index);
    const toggle = () => {
      const collapsed = !block.classList.contains('collapsed');
      if (collapsed) collapsedEpisodes.add(key);
      else collapsedEpisodes.delete(key);
      setCollapsed(block, header, collapsed);
    };
    header.dataset.episodeToggleBound = 'true';
    header.setAttribute('role', 'button');
    header.tabIndex = 0;
    setCollapsed(block, header, collapsedEpisodes.has(key));
    header.addEventListener('click', toggle);
    header.addEventListener('keydown', event => {
      if (event.key !== 'Enter' && event.key !== ' ') return;
      event.preventDefault();
      toggle();
    });
  });
}
