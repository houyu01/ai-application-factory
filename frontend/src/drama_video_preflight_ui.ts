/** Block video generation until the selected shot's prompt and references are usable. */
import type { ApiProject, DramaShot, GenerationTask } from './models.js';

type VideoPreflightRuntime = {
  apiBaseUrl: string;
  getActiveProject: () => ApiProject | null;
  getSelectedShot: (project: ApiProject) => DramaShot | undefined;
  onTaskCreated: (project: ApiProject, task: GenerationTask) => void;
  toast: (message: string) => void;
};

let runtime: VideoPreflightRuntime | null = null;

function referencedAssetIds(shot: DramaShot) {
  const fromPrompt = (shot.prompt_rich || [])
    .flatMap(node => node.type === 'reference' ? [node.asset_id] : []);
  return [...new Set(fromPrompt.length ? fromPrompt : shot.reference_asset_ids || [])];
}

function hasBoundaryFrames(shot: DramaShot) {
  const frames = shot.first_last_frames;
  return Boolean(frames?.first?.url || frames?.last?.url);
}

/** Mirror the durable version immediately so the history can show its pending card. */
function addPendingVideoVersion(shot: DramaShot, task: GenerationTask) {

  const versionId = task.input_snapshot?.version_id;
  if (typeof versionId !== 'string' || !versionId || (shot.versions || []).some(item => item.id === versionId)) return;
  const versions = shot.versions || [];
  const versionNo = Math.max(0, ...versions.map(item => Number(item.version_no || 0))) + 1;
  shot.versions = [{
    id: versionId,
    version_no: versionNo,
    task_id: task.id,
    status: task.status,
    progress: task.progress || 0,
    provider_task_id: task.provider_task_id,
    created_at: task.created_at || new Date().toISOString(),
  }, ...versions];
}

function prerequisiteIssues(project: ApiProject, shot: DramaShot) {
  const issues: string[] = [];
  if (!shot.prompt.trim()) issues.push('请先生成或保存分镜提示词。');
  const assets = new Map((project.assets || []).map(asset => [asset.id, asset]));
  referencedAssetIds(shot).forEach(assetId => {
    const asset = assets.get(assetId);
    if (!asset) issues.push(`引用素材“${assetId}”已不存在。`);
    else if (asset.status === '生成中') issues.push(`素材“${asset.name}”的图片仍在生成中。`);
    else if (asset.status === '生成失败') issues.push(`素材“${asset.name}”的图片生成失败，请重新生成或上传。`);
    else if (asset.status !== '生成成功' || !asset.image_url) issues.push(`素材“${asset.name}”尚未生成图片。`);
  });
  return issues;
}

function resetButton(button: HTMLButtonElement) {
  button.disabled = false;
  button.classList.remove('is-loading');
  button.setAttribute('aria-busy', 'false');
  button.innerHTML = '▣ 生成视频';
}

function errorIssues(message: string) {
  return message.split('\n')
    .map(line => line.replace(/^\s*-\s*/, '').trim())
    .filter(line => line && !line.startsWith('暂不能生成视频'));
}

function showPrerequisiteDialog(issues: string[]) {
  document.querySelector('[data-drama-video-prerequisite-dialog]')?.remove();
  const backdrop = document.createElement('div');
  backdrop.className = 'modal-backdrop drama-video-prerequisite-backdrop';
  backdrop.dataset.dramaVideoPrerequisiteDialog = 'true';
  backdrop.innerHTML = '<section class="modal drama-video-prerequisite-modal" role="dialog" aria-modal="true" aria-labelledby="drama-video-prerequisite-title"><button type="button" class="close" aria-label="关闭">×</button><div class="modal-head"><h2 id="drama-video-prerequisite-title">暂不能生成视频</h2><p>请先完成当前分镜的以下准备，再重新生成。</p></div><ul class="drama-video-prerequisite-list"></ul><div class="modal-actions"><button type="button" class="primary">我知道了</button></div></section>';
  const list = backdrop.querySelector<HTMLUListElement>('.drama-video-prerequisite-list')!;
  issues.forEach(issue => {
    const item = document.createElement('li');
    item.textContent = issue;
    list.append(item);
  });
  const close = () => backdrop.remove();
  backdrop.querySelectorAll<HTMLButtonElement>('.close,.primary').forEach(button => button.addEventListener('click', close));
  backdrop.addEventListener('click', event => { if (event.target === backdrop) close(); });
  document.body.append(backdrop);
}

async function responseDetail(response: Response) {
  const body = await response.json().catch(() => ({})) as { detail?: string };
  return body.detail || `HTTP ${response.status}`;
}

async function createVideoTask(project: ApiProject, shot: DramaShot, button: HTMLButtonElement) {
  if (!runtime) return;
  try {
    const response = await fetch(
      `${runtime.apiBaseUrl}/projects/${encodeURIComponent(project.id)}/shots/${encodeURIComponent(shot.id)}/video`,
      { method: 'POST' },
    );
    if (!response.ok) throw new Error(await responseDetail(response));
    const task = await response.json() as GenerationTask;
    addPendingVideoVersion(shot, task);
    runtime.onTaskCreated(project, task);
    runtime.toast('分镜视频任务已创建');
  } catch (error) {
    resetButton(button);
    const message = error instanceof Error ? error.message : '视频任务创建失败，请稍后重试。';
    const issues = errorIssues(message);
    showPrerequisiteDialog(issues.length ? issues : [message]);
  }
}

/** Configure the capture-phase preflight used by the shot Generate Video button. */
export function configureDramaVideoPreflight(value: VideoPreflightRuntime) {
  runtime = value;
}

document.addEventListener('click', event => {
  const target = event.target instanceof HTMLElement ? event.target : null;
  const button = target?.closest<HTMLButtonElement>('#drama-generate-shot-video');
  const project = runtime?.getActiveProject();
  const shot = project && runtime?.getSelectedShot(project);
  if (!button || button.disabled || !project || !shot) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  const issues = prerequisiteIssues(project, shot);
  if (issues.length) {
    resetButton(button);
    showPrerequisiteDialog(issues);
    return;
  }
  if (hasBoundaryFrames(shot)) {
    runtime?.toast('首尾帧会与当前参考图一并发送，并由提示词约束视频的起止画面。');
  }
  void createVideoTask(project, shot, button);
}, true);
