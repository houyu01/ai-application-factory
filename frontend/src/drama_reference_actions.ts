import type { ApiProject, DramaPromptNode, DramaShot } from './models.js';
import { dramaSelectedShot, dramaShotReferences, serializeDramaPromptNodes } from './drama_core_ui.js';
import { dramaViewState } from './drama_state.js';

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

function shotNodes(project: ApiProject, shot: DramaShot): DramaPromptNode[] {
  if (shot.prompt_rich?.length) return shot.prompt_rich;
  return [{ type: 'text', text: shot.prompt || '' }, ...dramaShotReferences(project, shot)];
}

function withoutReference(nodes: DramaPromptNode[], referenceIndex: number): DramaPromptNode[] {
  let index = -1;
  return nodes.filter(node => {
    if (node.type !== 'reference') return true;
    index += 1;
    return index !== referenceIndex;
  });
}

function selectedAssetIds(nodes: DramaPromptNode[]): string[] {
  return [...new Set(nodes.flatMap(node => node.type === 'reference' ? [node.asset_id] : []))];
}

/** Saves reference removal and starts missing-reference images without touching ready assets. */
export function configureDramaReferenceRemoval(runtime: Runtime) {
  document.addEventListener('click', event => {
    const target = event.target instanceof Element ? event.target : null;
    const button = target?.closest<HTMLButtonElement>('[data-drama-remove-reference]');
    const projectId = dramaViewState.projectId;
    if (!button || !projectId || button.disabled) return;
    const referenceIndex = Number(button.dataset.dramaRemoveReference);
    if (!Number.isInteger(referenceIndex) || referenceIndex < 0) return;
    event.preventDefault();
    event.stopPropagation();
    button.disabled = true;
    button.setAttribute('aria-busy', 'true');
    void (async () => {
      const projectResponse = await fetch(`${runtime.apiBaseUrl}/projects/${projectId}`);
      if (!projectResponse.ok) throw new Error(await responseDetail(projectResponse));
      const project = await projectResponse.json() as ApiProject;
      const shot = dramaSelectedShot(project);
      if (!shot) throw new Error('当前没有可编辑的分镜');
      const nodes = shotNodes(project, shot);
      const nextNodes = withoutReference(nodes, referenceIndex);
      if (nextNodes.length === nodes.length) throw new Error('参考图已更新，请刷新后重试');
      const serialized = serializeDramaPromptNodes(project, nextNodes);
      const saveResponse = await fetch(`${runtime.apiBaseUrl}/projects/${projectId}/shots/${shot.id}`, {
        method: 'PUT', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ prompt: serialized.prompt, prompt_rich: serialized.nodes, reference_asset_ids: selectedAssetIds(serialized.nodes) }),
      });
      if (!saveResponse.ok) throw new Error(await responseDetail(saveResponse));
      await runtime.reloadProject(projectId);
      runtime.notify('已从当前分镜移除参考图');
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
}
