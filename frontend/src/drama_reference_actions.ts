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

/** Clears the current shot's reference selection and prompt without deleting source assets. */
export function configureDramaReferenceRemoval(runtime: Runtime) {
  document.addEventListener('click', event => {
    const target = event.target instanceof Element ? event.target : null;
    const button = target?.closest<HTMLButtonElement>('[data-drama-remove-reference]');
    const projectId = dramaViewState.projectId;
    if (!button || !projectId || button.disabled) return;
    const shotId = button.dataset.dramaShotId || dramaViewState.shotId;
    if (!shotId) return;
    event.preventDefault();
    event.stopPropagation();
    button.disabled = true;
    button.setAttribute('aria-busy', 'true');
    void (async () => {
      const saveResponse = await fetch(`${runtime.apiBaseUrl}/projects/${projectId}/shots/${shotId}`, {
        method: 'PUT', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ prompt: '', prompt_rich: [], reference_asset_ids: [] }),
      });
      if (!saveResponse.ok) throw new Error(await responseDetail(saveResponse));
      await runtime.reloadProject(projectId);
      runtime.notify('已清空当前分镜的参考图和提示词');
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
    const idleText = button.textContent || '✣ 生成提示词';
    button.disabled = true;
    button.setAttribute('aria-busy', 'true');
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
      button.disabled = false;
      button.textContent = idleText;
      runtime.notify(`提示词生成失败：${error instanceof Error ? error.message : '请重试'}`);
    }).finally(() => button.removeAttribute('aria-busy'));
  }, true);
}
