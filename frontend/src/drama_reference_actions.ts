import { dramaViewState } from './drama_state.js';
import { dramaPromptNodes, readDramaPromptNodes, serializeDramaPromptNodes, setGenerationButtonLoading } from './drama_core_ui.js';
import { dramaReferenceAssetIds, removeDramaReference } from './drama_reference_removal.js';
import type { ApiProject, DramaPromptNode } from './models.js';
import { flushDramaEditorAutosave } from './drama_editor_autosave.js';

type Runtime = {
  apiBaseUrl: string;
  notify: (message: string) => void;
  reloadProject: (projectId: string) => Promise<void>;
};

function responseDetail(response: Response): Promise<string> {
  return response.json()
    .then((payload: { detail?: string }) => payload.detail || `HTTP ${response.status}`)
    .catch(() => `HTTP ${response.status}`);
}

function editablePromptNodes(project: ApiProject, shotId: string): DramaPromptNode[] {
  const editor = document.querySelector<HTMLElement>('.drama-rich-prompt-editor');
  if (editor) {
    const nodes = readDramaPromptNodes(editor);
    if (nodes.length) return nodes;
  }
  const promptInput = document.querySelector<HTMLTextAreaElement>('#drama-shot-prompt');
  try {
    const stored = JSON.parse(promptInput?.dataset.promptRich || '[]');
    if (Array.isArray(stored) && stored.length) return stored as DramaPromptNode[];
  } catch { /* Use the persisted rich prompt below. */ }
  const shot = project.shots?.find(item => item.id === shotId);
  return shot ? dramaPromptNodes(project, shot) : [];
}

/** Removes one current-shot reference while retaining all other prompt content and source assets. */
export function configureDramaReferenceRemoval(runtime: Runtime) {
  document.addEventListener('click', event => {
    const target = event.target instanceof Element ? event.target : null;
    const button = target?.closest<HTMLButtonElement>('[data-drama-remove-reference]');
    const projectId = dramaViewState.projectId;
    if (!button || !projectId || button.disabled) return;
    const shotId = button.dataset.dramaShotId || dramaViewState.shotId;
    const referenceAssetId = button.dataset.dramaReferenceAssetId;
    const referenceVariantId = button.dataset.dramaReferenceVariantId;
    if (!shotId || !referenceAssetId) return;
    event.preventDefault();
    event.stopPropagation();
    button.disabled = true;
    button.setAttribute('aria-busy', 'true');
    void (async () => {
      await flushDramaEditorAutosave();
      const projectResponse = await fetch(`${runtime.apiBaseUrl}/projects/${projectId}`);
      if (!projectResponse.ok) throw new Error(await responseDetail(projectResponse));
      const project = await projectResponse.json() as ApiProject;
      if (!project.shots?.some(shot => shot.id === shotId)) throw new Error('当前分镜不存在');
      const currentNodes = editablePromptNodes(project, shotId);
      const remainingNodes = removeDramaReference(currentNodes, referenceAssetId, referenceVariantId);
      const prompt = serializeDramaPromptNodes(project, remainingNodes);
      const saveResponse = await fetch(`${runtime.apiBaseUrl}/projects/${projectId}/shots/${shotId}`, {
        method: 'PUT', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ prompt: prompt.prompt, prompt_rich: prompt.nodes, reference_asset_ids: dramaReferenceAssetIds(prompt.nodes) }),
      });
      if (!saveResponse.ok) throw new Error(await responseDetail(saveResponse));
      await runtime.reloadProject(projectId);
      runtime.notify('已移除当前分镜中的参考图');
    })().catch(error => {
      console.error(error);
      runtime.notify(`移除参考图失败：${error instanceof Error ? error.message : '请重试'}`);
    }).finally(() => {
      button.disabled = false;
      button.removeAttribute('aria-busy');
    });
  }, true);

  document.addEventListener('click', event => {
    const target = event.target instanceof Element ? event.target : null;
    const button = target?.closest<HTMLButtonElement>('[data-drama-generate-reference-images]');
    const projectId = dramaViewState.projectId;
    const shotId = dramaViewState.shotId
      || document.querySelector<HTMLElement>('.drama-shot-item.selected [data-drama-shot]')?.dataset.dramaShot;
    if (!button || !projectId || !shotId || button.disabled) return;
    event.preventDefault();
    event.stopPropagation();
    button.disabled = true;
    button.setAttribute('aria-busy', 'true');
    button.textContent = '⟳ 正在创建任务…';
    dramaViewState.shotId = shotId;
    void fetch(`${runtime.apiBaseUrl}/projects/${projectId}/shots/${shotId}/reference-images/generate`, { method: 'POST' })
      .then(async response => {
        if (!response.ok) throw new Error(await responseDetail(response));
        await runtime.reloadProject(projectId);
        runtime.notify('已开始批量生成未生成的参考图');
      })
      .catch(error => {
        console.error(error);
        button.textContent = '一键生成参考图';
        runtime.notify(`参考图批量生成失败：${error instanceof Error ? error.message : '请重试'}`);
      })
      .finally(() => {
        button.disabled = false;
        button.removeAttribute('aria-busy');
      });
  }, true);

  document.addEventListener('click', event => {
    const target = event.target instanceof Element ? event.target : null;
    const button = target?.closest<HTMLButtonElement>('#drama-generate-shot-prompt');
    const projectId = dramaViewState.projectId;
    const shotId = dramaViewState.shotId;
    if (!button || !projectId || !shotId) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    const title = document.querySelector<HTMLInputElement>('#drama-shot-title')?.value || '';
    const originalText = document.querySelector<HTMLTextAreaElement>('#drama-shot-original')?.value || '';
    const idleText = button.dataset.taskIdleText || '✣ 生成提示词';
    setGenerationButtonLoading(button, true, idleText);
    void (async () => {
      const saveResponse = await fetch(`${runtime.apiBaseUrl}/projects/${projectId}/shots/${shotId}`, {
        method: 'PUT', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ title, original_text: originalText }),
      });
      if (!saveResponse.ok) throw new Error(await responseDetail(saveResponse));
      const taskResponse = await fetch(`${runtime.apiBaseUrl}/projects/${projectId}/shots/${shotId}/prompt`, { method: 'POST' });
      if (!taskResponse.ok) throw new Error(await responseDetail(taskResponse));
      runtime.notify('分镜提示词任务已创建，正按当前剧本匹配已生成参考图');
      await runtime.reloadProject(projectId);
    })().catch(error => {
      console.error(error);
      setGenerationButtonLoading(button, false, idleText);
      runtime.notify(`提示词生成失败：${error instanceof Error ? error.message : '请重试'}`);
    }).finally(() => button.removeAttribute('aria-busy'));
  }, true);
}
