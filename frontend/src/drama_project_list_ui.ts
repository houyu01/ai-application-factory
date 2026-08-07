/** Project-library cards, including durable bootstrap queue presentation. */

import type { ApiProject, DramaAsset, Locale, Project } from './models.js';

type DramaListCopy = 'workspace' | 'dramaTitle' | 'dramaDescription' | 'newDrama' | 'search' | 'refresh' | 'projects';

type DramaListOptions = {
  projects: Project[];
  locale: Locale;
  ui: (key: DramaListCopy) => string;
  escapeHtml: (value: unknown) => string;
  resolveMediaUrl: (value?: string | null) => string;
};

function firstProjectCoverUrl(assets: DramaAsset[]) {
  for (const cover of assets.filter(asset => asset.type === 'cover').sort((left, right) => String(left.created_at || '').localeCompare(String(right.created_at || '')) || left.id.localeCompare(right.id))) {
    const url = (cover.image_history || []).map(item => item.url).find((item): item is string => typeof item === 'string' && Boolean(item.trim())) || cover.image_url;
    if (url?.trim()) return url;
  }
  return null;
}

export function projectFromApi(project: ApiProject): Project {
  const assets = project.assets || [];
  return {
    id: project.id, name: project.name, status: project.status, ratio: project.ratio,
    style: project.style, theme: project.theme,
    createdAt: project.created_at?.slice(0, 16).replace('T', ' ') || '刚刚',
    scenes: project.shots?.length || 0,
    characters: assets.filter(asset => asset.type === 'character').length,
    locations: assets.filter(asset => asset.type === 'scene').length,
    props: assets.filter(asset => asset.type === 'prop').length,
    coverUrl: firstProjectCoverUrl(assets),
    queuePosition: project.queue_position,
    queueState: project.queue_state,
  };
}

function isGenerating(project: Project) {
  return project.status === '生成中';
}

function sortedProjects(projects: Project[]) {
  return [...projects].sort((left, right) => {
    const leftQueued = isGenerating(left);
    const rightQueued = isGenerating(right);
    if (leftQueued !== rightQueued) return leftQueued ? -1 : 1;
    if (leftQueued && rightQueued) {
      return (left.queuePosition ?? Number.MAX_SAFE_INTEGER) - (right.queuePosition ?? Number.MAX_SAFE_INTEGER)
        || left.createdAt.localeCompare(right.createdAt);
    }
    return right.createdAt.localeCompare(left.createdAt);
  });
}

function projectStatusText(project: Project, locale: Locale) {
  if (!isGenerating(project)) {
    const labels: Record<string, string> = locale === 'en'
      ? { 草稿: 'Draft', 生成成功: 'Succeeded', 生成失败: 'Failed', 已取消: 'Cancelled' }
      : {};
    return labels[project.status] || project.status;
  }
  const position = project.queuePosition || 1;
  const waitingForWorker = project.queueState === 'queued';
  if (locale === 'en') {
    return waitingForWorker
      ? `Queued · Queue position #${position}`
      : `Generating · Queue position #${position}`;
  }
  return waitingForWorker
    ? `排队中，当前排在第${position}位`
    : `生成中，当前排在第${position}位`;
}

function projectCard(project: Project, options: DramaListOptions) {
  const { escapeHtml, locale, resolveMediaUrl } = options;
  const cover = project.coverUrl ? `<img class="project-cover-background" src="${escapeHtml(resolveMediaUrl(project.coverUrl))}" alt="" aria-hidden="true" />` : '';
  return `<article class="project-card" data-project="${project.id}">${cover}<div class="card-top"><h2>${escapeHtml(project.name)}</h2><span class="status ${isGenerating(project) ? 'running' : ''}">${isGenerating(project) ? '◌ ' : ''}${escapeHtml(projectStatusText(project, locale))}</span><div class="tags"><span>${escapeHtml(project.ratio)}</span><span>${escapeHtml(project.style)}</span><span>${escapeHtml(project.theme)}</span></div></div><div class="metrics"><div><strong>${project.scenes}</strong><small>${locale === 'en' ? 'Shots' : '分镜'}</small></div><div><strong>${project.characters}</strong><small>${locale === 'en' ? 'Roles' : '角色'}</small></div><div><strong>${project.locations}</strong><small>${locale === 'en' ? 'Scenes' : '场景'}</small></div><div><strong>${project.props}</strong><small>${locale === 'en' ? 'Props' : '道具'}</small></div></div><div class="card-foot"><span>${escapeHtml(project.createdAt)}</span><button type="button" class="delete-card-button" data-delete-project="${project.id}">删除</button></div></article>`;
}

export function dramaProjectListPage(options: DramaListOptions) {
  const { projects, ui } = options;
  return `<header><div><div class="eyebrow">${ui('workspace')}</div><h1>${ui('dramaTitle')}</h1><p>${ui('dramaDescription')}</p></div><button class="primary" id="new-project">${ui('newDrama')}</button></header><section class="toolbar"><div class="search">⌕ <input placeholder="${ui('search')}" /></div><button class="ghost">${ui('refresh')}</button><span class="toolbar-count">${projects.length} ${ui('projects')}</span></section><section class="cards">${sortedProjects(projects).map(project => projectCard(project, options)).join('')}</section>`;
}
