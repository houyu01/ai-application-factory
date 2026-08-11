/** Render game-node video versions with the same preview, refine, download, and delete affordances as drama videos. */

import { confirmAction } from './confirmation_modal.js';
import { gameNodeVideoHistoryRecords, gameNodeVideoHistoryTime, selectGameNodeVideoUrl, selectedGameNodeVideoId, selectedGameNodeVideoUrl, type GameNodeVideoRecord } from './game_node_video_history.js';
import type { Game, GameNode, GameTask } from './models.js';
import { icon } from './ui_icons.js';
import './game_node_video_history.css';

type Options = {
  apiBaseUrl: string;
  game: Game;
  inspector: HTMLElement;
  node: GameNode;
  resolveMediaUrl: (value?: string | null) => string;
  task?: GameTask;
  toast: (message: string) => void;
  refresh: () => Promise<void>;
};

function statusLabel(record: GameNodeVideoRecord) {
  if (record.status === '生成中') return `生成中 ${Math.max(0, Number(record.progress || 0))}%`;
  if (record.status === '生成成功') return '成功';
  if (record.status === '生成失败') return '失败';
  return record.status || '未生成';
}

function statusClass(record: GameNodeVideoRecord) {
  if (record.status === '生成中') return 'running';
  if (record.status === '生成成功') return 'success';
  if (record.status === '生成失败') return 'failed';
  return '';
}

function responseMessage(response: Response): Promise<string> {
  return response.json()
    .then((value: { detail?: string }) => value.detail || `HTTP ${response.status}`)
    .catch(() => `HTTP ${response.status}`);
}

function selectPreview(options: Options, record: GameNodeVideoRecord, entry: HTMLElement) {
  if (!record.url) return;
  selectGameNodeVideoUrl(options.node, record.url);
  const player = options.inspector.querySelector<HTMLVideoElement>('.game-node-video-player');
  if (player) {
    player.src = options.resolveMediaUrl(record.url);
    player.load();
  }
  options.inspector.querySelectorAll('.game-node-history-entry.is-selected').forEach(item => item.classList.remove('is-selected'));
  entry.classList.add('is-selected');
}

async function selectVersionForUse(options: Options, record: GameNodeVideoRecord, button: HTMLButtonElement) {
  button.disabled = true;
  try {
    const response = await fetch(
      `${options.apiBaseUrl}/games/${encodeURIComponent(options.game.id)}/nodes/${encodeURIComponent(options.node.id)}/videos/${encodeURIComponent(record.id)}/use-selection`,
      { method: 'PUT' },
    );
    if (!response.ok) throw new Error(await responseMessage(response));
    selectGameNodeVideoUrl(options.node, record.url);
    options.toast('已设为当前使用版本，编辑器预览和试玩都会使用它');
    await options.refresh();
  } catch (error) {
    button.disabled = false;
    options.toast(error instanceof Error ? error.message : '设置使用版本失败');
  }
}

async function deleteVersion(options: Options, record: GameNodeVideoRecord, button: HTMLButtonElement) {
  if (!await confirmAction({ title: '删除视频历史？', description: '对应视频文件和生成记录会一并删除，且无法恢复。', confirmLabel: '删除视频' })) return;
  button.disabled = true;
  try {
    const response = await fetch(
      `${options.apiBaseUrl}/games/${encodeURIComponent(options.game.id)}/nodes/${encodeURIComponent(options.node.id)}/videos/${encodeURIComponent(record.id)}`,
      { method: 'DELETE' },
    );
    if (!response.ok) throw new Error(await responseMessage(response));
    if (selectedGameNodeVideoUrl(options.node) === record.url) selectGameNodeVideoUrl(options.node, null);
    options.toast('视频历史已删除');
    await options.refresh();
  } catch (error) {
    button.disabled = false;
    options.toast(error instanceof Error ? error.message : '删除视频历史失败');
  }
}

function openRefinementModal(options: Options, record: GameNodeVideoRecord) {
  if (!record.id || !record.url) {
    options.toast('该视频记录无法微调');
    return;
  }
  const backdrop = document.createElement('div');
  backdrop.className = 'modal-backdrop game-node-video-refinement-backdrop';
  const modal = document.createElement('section');
  modal.className = 'modal game-node-video-refinement-modal';
  modal.setAttribute('role', 'dialog');
  modal.setAttribute('aria-modal', 'true');
  const close = () => backdrop.remove();
  modal.innerHTML = `<button type="button" class="close" aria-label="关闭">×</button><div class="modal-head"><h2>视频微调</h2><p>会携带当前历史视频、其原始提示词和参考图，按补充说明生成一个新版本。</p></div><video class="game-node-video-refinement-preview" controls playsinline></video><label class="game-node-video-refinement-field"><span>微调提示词</span><textarea rows="5" maxlength="4000" placeholder="描述需要修改的内容，例如镜头、动作、光线或节奏。"></textarea></label><div class="modal-actions"><button type="button" class="ghost" data-game-refinement-cancel>取消</button><button type="button" class="primary" data-game-refinement-submit>新增生成</button></div>`;
  const preview = modal.querySelector<HTMLVideoElement>('video')!;
  preview.src = options.resolveMediaUrl(record.url);
  const input = modal.querySelector<HTMLTextAreaElement>('textarea')!;
  input.value = record.refinementPrompt || '';
  const submit = modal.querySelector<HTMLButtonElement>('[data-game-refinement-submit]')!;
  modal.querySelector('.close')?.addEventListener('click', close);
  modal.querySelector('[data-game-refinement-cancel]')?.addEventListener('click', close);
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
        `${options.apiBaseUrl}/games/${encodeURIComponent(options.game.id)}/nodes/${encodeURIComponent(options.node.id)}/videos/${encodeURIComponent(record.id)}/refinement`,
        { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ refinement_prompt: refinementPrompt }) },
      );
      if (!response.ok) throw new Error(await responseMessage(response));
      close();
      options.toast('视频微调任务已创建');
      await options.refresh();
    } catch (error) {
      submit.disabled = false;
      submit.textContent = '新增生成';
      options.toast(error instanceof Error ? error.message : '视频微调任务创建失败');
    }
  });
  backdrop.append(modal);
  document.body.append(backdrop);
  input.focus();
}

function historyEntry(options: Options, record: GameNodeVideoRecord, index: number) {
  const entry = document.createElement('article');
  entry.className = `game-node-history-entry ${record.status === '生成失败' ? 'is-failed' : ''}`;
  entry.classList.toggle('is-selected', Boolean(record.url) && record.url === selectedGameNodeVideoUrl(options.node));
  const preview = document.createElement(record.url && record.status !== '生成失败' ? 'button' : 'div');
  preview.className = 'game-node-history-preview';
  if (preview instanceof HTMLButtonElement) {
    preview.type = 'button';
    preview.title = '预览视频';
    preview.innerHTML = `<video muted playsinline preload="metadata" src="${options.resolveMediaUrl(record.url)}"></video>`;
    preview.addEventListener('click', () => selectPreview(options, record, entry));
  } else if (record.status === '生成中') preview.innerHTML = '<span class="generation-spinner" aria-hidden="true"></span>';
  else if (record.status === '生成失败') preview.innerHTML = '<span class="game-node-history-failed-icon">!</span>';
  else preview.innerHTML = icon('history');
  const details = document.createElement('div');
  details.className = 'game-node-history-details';
  const createdAt = gameNodeVideoHistoryTime(record.createdAt);
  details.innerHTML = `<strong>v${Math.max(1, index)}</strong><span class="status ${statusClass(record)}">${statusLabel(record)}</span>${createdAt ? `<small>${createdAt}</small>` : ''}`;
  const actions = document.createElement('div');
  actions.className = 'game-node-history-actions';
  if (record.status === '生成失败' && record.error) {
    const error = document.createElement('button');
    error.type = 'button';
    error.className = 'game-node-history-error';
    error.title = record.error;
    error.setAttribute('aria-label', `查看失败原因：${record.error}`);
    error.innerHTML = icon('info');
    actions.append(error);
  }
  if (record.url && record.status === '生成成功') {
    const useVersion = document.createElement('button');
    useVersion.type = 'button';
    useVersion.className = 'game-node-history-use-version';
    useVersion.disabled = selectedGameNodeVideoId(options.node) === record.id;
    useVersion.title = useVersion.disabled ? '当前使用版本' : '设为当前使用版本';
    useVersion.setAttribute('aria-label', useVersion.title);
    useVersion.textContent = '✓';
    useVersion.addEventListener('click', () => void selectVersionForUse(options, record, useVersion));
    const download = document.createElement('a');
    download.className = 'game-node-history-download';
    download.href = options.resolveMediaUrl(record.url);
    download.download = 'game-node-video.mp4';
    download.target = '_blank';
    download.rel = 'noopener';
    download.title = '下载视频';
    download.setAttribute('aria-label', '下载视频');
    download.innerHTML = icon('download');
    const refine = document.createElement('button');
    refine.type = 'button';
    refine.className = 'game-node-history-refine';
    refine.title = '微调视频';
    refine.setAttribute('aria-label', '微调视频');
    refine.innerHTML = icon('wrench');
    refine.addEventListener('click', () => openRefinementModal(options, record));
    actions.append(useVersion, download, refine);
  }
  if (record.status !== '生成中' && record.id) {
    const remove = document.createElement('button');
    remove.type = 'button';
    remove.className = 'game-node-history-delete';
    remove.title = '删除视频历史';
    remove.setAttribute('aria-label', '删除视频历史');
    remove.innerHTML = icon('trash');
    remove.addEventListener('click', () => void deleteVersion(options, record, remove));
    actions.append(remove);
  }
  entry.append(preview, details, actions);
  return entry;
}

/** Replace the inspector's legacy history buttons with durable generation states and version actions. */
export function syncGameNodeVideoHistory(options: Options) {
  const history = options.inspector.querySelector<HTMLElement>('[data-game-node-video-history]');
  if (!history) return;
  const records = gameNodeVideoHistoryRecords(options.node, options.task);
  const signature = JSON.stringify([options.node.selected_video_id, records.map(item => [item.id, item.status, item.progress, item.url, item.error])]);
  if (history.dataset.historySignature === signature) return;
  history.dataset.historySignature = signature;
  const heading = document.createElement('div');
  heading.className = 'game-node-video-history-head';
  heading.innerHTML = `<h4>视频历史</h4><span>${records.length} 个版本</span>`;
  const scroll = document.createElement('div');
  scroll.className = 'game-node-history-scroll';
  if (records.length) records.forEach((record, index) => scroll.append(historyEntry(options, record, records.length - index)));
  else scroll.innerHTML = '<p class="game-node-video-history-empty">暂无历史视频</p>';
  history.replaceChildren(heading, scroll);
}
