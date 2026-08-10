/** Persist the visible drama editor draft before refreshes can replace the DOM. */
import type { ApiProject, DramaPromptNode, DramaShot } from './models.js';

export type DramaEditorSnapshot = {
  projectId: string;
  shotId: string;
  projectName: string;
  title: string;
  originalText: string;
  prompt: string;
  promptRich: DramaPromptNode[];
  durationSeconds: number;
};

type SerializedPrompt = { prompt: string; nodes: DramaPromptNode[] };
type AutosaveRuntime = {
  apiBaseUrl: string;
  getActiveProject: () => ApiProject | null;
  getSelectedShot: (project: ApiProject) => DramaShot | undefined;
  readPromptNodes: (root: HTMLElement) => DramaPromptNode[];
  serializePrompt: (project: ApiProject, nodes: DramaPromptNode[]) => SerializedPrompt;
  toast: (message: string) => void;
};

const SAVE_DELAY_MS = 700;
let runtime: AutosaveRuntime | null = null;
let timer: number | undefined;
let saveQueue = Promise.resolve();
const shotSnapshots = new Map<string, DramaEditorSnapshot>();

function shotKey(projectId: string, shotId: string) {
  return `${projectId}:${shotId}`;
}

function sameNodes(left: DramaPromptNode[], right: DramaPromptNode[]) {
  return JSON.stringify(left) === JSON.stringify(right);
}

/** Return only the project fields whose visible value differs from the saved snapshot. */
export function changedProjectPayload(draft: DramaEditorSnapshot, saved?: DramaEditorSnapshot) {
  const name = draft.projectName.trim();
  return name && name !== saved?.projectName ? { name } : null;
}

/** Return only the shot fields whose visible value differs from the saved snapshot. */
export function changedShotPayload(draft: DramaEditorSnapshot, saved?: DramaEditorSnapshot) {
  const changes: Record<string, unknown> = {};
  if (!saved || draft.title !== saved.title) changes.title = draft.title;
  if (!saved || draft.originalText !== saved.originalText) changes.original_text = draft.originalText;
  if (!saved || draft.prompt !== saved.prompt) changes.prompt = draft.prompt;
  if (!saved || !sameNodes(draft.promptRich, saved.promptRich)) changes.prompt_rich = draft.promptRich;
  if (!saved || draft.durationSeconds !== saved.durationSeconds) changes.duration_seconds = draft.durationSeconds;
  return Object.keys(changes).length ? changes : null;
}

function snapshotFromShot(project: ApiProject, shot: DramaShot): DramaEditorSnapshot {
  return {
    projectId: project.id,
    shotId: shot.id,
    projectName: project.name,
    title: shot.title || '',
    originalText: shot.original_text || '',
    prompt: shot.prompt || '',
    promptRich: Array.isArray(shot.prompt_rich) ? shot.prompt_rich : [],
    durationSeconds: Number(shot.duration_seconds || 10),
  };
}

function parseStoredNodes(input: HTMLTextAreaElement | null, fallback: DramaPromptNode[]) {
  try {
    const nodes = JSON.parse(input?.dataset.promptRich || '');
    return Array.isArray(nodes) ? nodes as DramaPromptNode[] : fallback;
  } catch {
    return fallback;
  }
}

function captureVisibleDraft(): DramaEditorSnapshot | null {
  const project = runtime?.getActiveProject();
  const shot = project && runtime?.getSelectedShot(project);
  const promptInput = document.querySelector<HTMLTextAreaElement>('#drama-shot-prompt');
  if (!runtime || !project || !shot || promptInput?.dataset.dramaPromptShotId !== shot.id) return null;
  const editor = document.querySelector<HTMLElement>('.drama-rich-prompt-editor');
  const nodes = editor ? runtime.readPromptNodes(editor) : parseStoredNodes(promptInput, shot.prompt_rich || []);
  const serialized = runtime.serializePrompt(project, nodes);
  promptInput.value = serialized.prompt;
  promptInput.dataset.promptRich = JSON.stringify(serialized.nodes);
  const duration = document.querySelector<HTMLSelectElement>('#drama-shot-duration');
  return {
    ...snapshotFromShot(project, shot),
    projectName: document.querySelector<HTMLInputElement>('#drama-project-name')?.value || project.name,
    title: document.querySelector<HTMLInputElement>('#drama-shot-title')?.value || '',
    originalText: document.querySelector<HTMLTextAreaElement>('#drama-shot-original')?.value || '',
    prompt: serialized.prompt,
    promptRich: serialized.nodes,
    durationSeconds: Number(duration?.value || shot.duration_seconds || 10),
  };
}

function rememberDraft(draft: DramaEditorSnapshot, projectPayload: { name: string } | null, shotPayload: Record<string, unknown> | null) {
  const key = shotKey(draft.projectId, draft.shotId);
  const previous = shotSnapshots.get(key) || draft;
  const saved = { ...previous };
  if (projectPayload) saved.projectName = projectPayload.name;
  if (shotPayload) {
    if ('title' in shotPayload) saved.title = draft.title;
    if ('original_text' in shotPayload) saved.originalText = draft.originalText;
    if ('prompt' in shotPayload) saved.prompt = draft.prompt;
    if ('prompt_rich' in shotPayload) saved.promptRich = draft.promptRich;
    if ('duration_seconds' in shotPayload) saved.durationSeconds = draft.durationSeconds;
  }
  shotSnapshots.set(key, saved);
  const project = runtime?.getActiveProject();
  const shot = project && runtime?.getSelectedShot(project);
  if (project?.id === draft.projectId && projectPayload) project.name = projectPayload.name;
  if (shot?.id === draft.shotId && shotPayload) {
    Object.assign(shot, {
      title: saved.title,
      original_text: saved.originalText,
      prompt: saved.prompt,
      prompt_rich: saved.promptRich,
      duration_seconds: saved.durationSeconds,
    });
  }
  document.querySelector<HTMLElement>('.drama-rich-prompt-editor')?.removeAttribute('data-video-inputs-dirty');
}

async function saveDraft(draft: DramaEditorSnapshot) {
  const saved = shotSnapshots.get(shotKey(draft.projectId, draft.shotId));
  const projectPayload = changedProjectPayload(draft, saved);
  const shotPayload = changedShotPayload(draft, saved);
  if (!projectPayload && !shotPayload) return;
  const requests: Promise<Response>[] = [];
  if (projectPayload) requests.push(fetch(`${runtime!.apiBaseUrl}/projects/${draft.projectId}/name`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(projectPayload) }));
  if (shotPayload) requests.push(fetch(`${runtime!.apiBaseUrl}/projects/${draft.projectId}/shots/${draft.shotId}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(shotPayload) }));
  const responses = await Promise.all(requests);
  const failed = responses.find(response => !response.ok);
  if (failed) throw new Error(`HTTP ${failed.status}`);
  rememberDraft(draft, projectPayload, shotPayload);
}

async function drainDrafts(initial: DramaEditorSnapshot | null) {
  let draft = initial;
  while (draft) {
    await saveDraft(draft);
    const next = captureVisibleDraft();
    if (!next || shotKey(next.projectId, next.shotId) !== shotKey(draft.projectId, draft.shotId)) return;
    const saved = shotSnapshots.get(shotKey(next.projectId, next.shotId));
    if (!changedProjectPayload(next, saved) && !changedShotPayload(next, saved)) return;
    draft = next;
  }
}

/** Flush pending editor changes now; callers should await this before a DOM-refreshing action. */
export function flushDramaEditorAutosave() {
  if (timer !== undefined) window.clearTimeout(timer);
  timer = undefined;
  const draft = captureVisibleDraft();
  const task = saveQueue.then(() => drainDrafts(draft));
  saveQueue = task.catch(error => {
    runtime?.toast('自动保存失败，内容仍保留在当前页面，请稍后重试');
    console.error(error);
  });
  return task;
}

/** Schedule a no-op-safe, debounced save after a visible editor field changes. */
export function scheduleDramaEditorAutosave() {
  if (timer !== undefined) window.clearTimeout(timer);
  timer = window.setTimeout(() => { void flushDramaEditorAutosave().catch(() => undefined); }, SAVE_DELAY_MS);
}

/** True when the selected editor has changes that have not reached SQLite yet. */
export function hasUnsavedDramaEditorChanges() {
  const draft = captureVisibleDraft();
  if (!draft) return false;
  const saved = shotSnapshots.get(shotKey(draft.projectId, draft.shotId));
  return Boolean(changedProjectPayload(draft, saved) || changedShotPayload(draft, saved));
}

/** Seed the change detector from a newly loaded project without issuing a save request. */
export function registerDramaEditorAutosave(project: ApiProject) {
  const shot = runtime?.getSelectedShot(project);
  if (!shot) return;
  const key = shotKey(project.id, shot.id);
  const received = snapshotFromShot(project, shot);
  const saved = shotSnapshots.get(key);
  if (!saved || (!changedProjectPayload(received, saved) && !changedShotPayload(received, saved))) {
    shotSnapshots.set(key, received);
  }
}

/** Configure global input listeners for the project title and selected-shot editor. */
export function configureDramaEditorAutosave(value: AutosaveRuntime) {
  runtime = value;
  document.addEventListener('input', event => {
    const target = event.target instanceof HTMLElement ? event.target : null;
    if (target?.matches('#drama-project-name,#drama-shot-title,#drama-shot-original,.drama-rich-prompt-editor')) scheduleDramaEditorAutosave();
  });
  document.addEventListener('change', event => {
    const target = event.target instanceof HTMLElement ? event.target : null;
    if (target?.matches('#drama-shot-duration')) scheduleDramaEditorAutosave();
  });
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') void flushDramaEditorAutosave().catch(() => undefined);
  });
}
