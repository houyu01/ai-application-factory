/** Video-refinement dialog for turning one completed historical video into a new durable generation. */
import type { ApiProject, DramaShot } from './models.js';
import type { DramaVideoHistoryRecord } from './drama_video_history.js';
import './drama_video_refinement.css';

type VideoRefinementModalOptions = {
  apiBaseUrl: string;
  project: ApiProject;
  record: DramaVideoHistoryRecord;
  resolveMediaUrl: (value?: string | null) => string;
  reloadProject: (id: string) => Promise<void>;
  shot: DramaShot;
  toast: (message: string) => void;
};

function responseMessage(response: Response): Promise<string> {
  return response.json()
    .then((value: { detail?: string }) => value.detail || `HTTP ${response.status}`)
    .catch(() => `HTTP ${response.status}`);
}

/** Open the history-version refinement dialog with previously saved feedback ready for another pass. */
export function openDramaVideoRefinementModal(options: VideoRefinementModalOptions) {
  if (!options.record.id || !options.record.url) {
    options.toast('该视频记录无法微调');
    return;
  }
  const backdrop = document.createElement('div');
  backdrop.className = 'modal-backdrop drama-video-refinement-backdrop';
  const modal = document.createElement('section');
  modal.className = 'modal drama-video-refinement-modal';
  modal.setAttribute('role', 'dialog');
  modal.setAttribute('aria-modal', 'true');
  const close = () => backdrop.remove();
  const closeButton = document.createElement('button');
  closeButton.type = 'button';
  closeButton.className = 'close';
  closeButton.setAttribute('aria-label', '关闭');
  closeButton.textContent = '×';
  const heading = document.createElement('div');
  heading.className = 'modal-head';
  const title = document.createElement('h2');
  title.textContent = '视频微调';
  const description = document.createElement('p');
  description.textContent = '会携带当前历史视频、其原始提示词和参考图，按你的补充说明生成一个新版本。';
  heading.append(title, description);
  const preview = document.createElement('video');
  preview.className = 'drama-video-refinement-preview';
  preview.controls = true;
  preview.playsInline = true;
  preview.src = options.resolveMediaUrl(options.record.url);
  const label = document.createElement('label');
  label.className = 'drama-video-refinement-field';
  const labelText = document.createElement('span');
  labelText.textContent = '微调提示词';
  const input = document.createElement('textarea');
  input.rows = 5;
  input.maxLength = 4_000;
  input.placeholder = '用户可在此输入对当前视频不满意的地方或需要补充的提示词';
  input.value = options.record.refinementPrompt || '';
  label.append(labelText, input);
  const actions = document.createElement('div');
  actions.className = 'modal-actions';
  const cancel = document.createElement('button');
  cancel.type = 'button';
  cancel.className = 'ghost';
  cancel.textContent = '取消';
  const submit = document.createElement('button');
  submit.type = 'button';
  submit.className = 'primary';
  submit.textContent = '新增生成';
  actions.append(cancel, submit);
  modal.append(closeButton, heading, preview, label, actions);
  backdrop.append(modal);
  document.body.append(backdrop);
  closeButton.addEventListener('click', close);
  cancel.addEventListener('click', close);
  backdrop.addEventListener('click', event => { if (event.target === backdrop) close(); });
  submit.addEventListener('click', async () => {
    const refinementPrompt = input.value.trim();
    if (!refinementPrompt) {
      options.toast('请填写微调提示词');
      input.focus();
      return;
    }
    submit.disabled = true;
    submit.textContent = '创建中…';
    try {
      const response = await fetch(
        `${options.apiBaseUrl}/projects/${encodeURIComponent(options.project.id)}/shots/${encodeURIComponent(options.shot.id)}/videos/${encodeURIComponent(options.record.id)}/refinement`,
        { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ refinement_prompt: refinementPrompt }) },
      );
      if (!response.ok) throw new Error(await responseMessage(response));
      close();
      options.toast('视频微调任务已创建');
      await options.reloadProject(options.project.id);
    } catch (error) {
      submit.disabled = false;
      submit.textContent = '新增生成';
      options.toast(error instanceof Error ? error.message : '视频微调任务创建失败');
    }
  });
  input.focus();
}
