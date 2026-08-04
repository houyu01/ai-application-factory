import type { ApiProject, DramaAssetKind } from './models.js';

/** Shared editor selection state so the drama UI modules render the same project and shot. */
export type DramaViewState = {
  projectId: string | null;
  shotId: string | null;
  assetPanel: DramaAssetKind | null;
  videoUrl: string | null;
};

export const dramaViewState: DramaViewState = { projectId: null, shotId: null, assetPanel: null, videoUrl: null };
export let activeDramaProject: ApiProject | null = null;
export function setActiveDramaProject(project: ApiProject | null) { activeDramaProject = project; }
