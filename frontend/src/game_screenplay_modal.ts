/** Screenplay editor actions for an interactive game, including full regeneration. */

import type { Game } from './models.js';
import { confirmAction } from './confirmation_modal.js';

type GameScreenplayModalOptions = {
  apiBaseUrl: string;
  game: Game;
  escapeHtml: (value: unknown) => string;
  toast: (message: string) => void;
  replaceGame: (game: Game) => void;
  refreshGame: (game?: Game) => Promise<void>;
};

async function responseError(response: Response) {
  const body = await response.json().catch(() => null) as { detail?: unknown; message?: unknown } | null;
  return typeof body?.detail === 'string' ? body.detail : typeof body?.message === 'string' ? body.message : `HTTP ${response.status}`;
}

/** Open the game screenplay dialog and retain its task-progress refresh while it is visible. */
export function openGameScreenplayModal(options: GameScreenplayModalOptions) {
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop';
  modal.innerHTML = `<div class="modal drama-expanded-script-modal" role="dialog" aria-modal="true" aria-labelledby="game-screenplay-title"><button class="close" aria-label="关闭">×</button><div class="modal-head"><h2 id="game-screenplay-title">分支剧本</h2><p data-game-screenplay-meta>正在加载剧本…</p></div><div class="drama-expanded-script-fields"><label><span>原始剧本</span><textarea id="game-original-script" rows="9">${options.escapeHtml(options.game.script)}</textarea></label><label><span>扩写后分支剧本（剧情段、抉择、条件、流向、结局）</span><textarea id="game-expanded-script" rows="16">${options.escapeHtml(options.game.expanded_script || '')}</textarea></label></div><div class="video-prompt-actions drama-expanded-script-actions"><div class="drama-expanded-script-action-group"><button class="ghost" id="game-expand-screenplay">扩写剧本</button><button class="ghost danger-button" id="game-regenerate-screenplay" title="清空当前扩写、图谱、素材和试玩记录，从原始剧本重新生成">重新生成</button><button class="ghost danger-button" id="game-cancel-screenplay" hidden>停止扩写</button></div><div class="drama-expanded-script-action-group"><button class="ghost game-screenplay-close">关闭</button><button class="primary" id="save-game-screenplay">保存修改</button></div></div></div>`;
  document.body.append(modal);
  let current = options.game;
  let refreshTimer: number | undefined;
  let loading = false;
  let regenerating = false;
  let originalDirty = false;
  const original = modal.querySelector<HTMLTextAreaElement>('#game-original-script')!;
  const expanded = modal.querySelector<HTMLTextAreaElement>('#game-expanded-script')!;
  const meta = modal.querySelector<HTMLElement>('[data-game-screenplay-meta]')!;
  const expandButton = modal.querySelector<HTMLButtonElement>('#game-expand-screenplay')!;
  const regenerateButton = modal.querySelector<HTMLButtonElement>('#game-regenerate-screenplay')!;
  const cancelButton = modal.querySelector<HTMLButtonElement>('#game-cancel-screenplay')!;
  const saveButton = modal.querySelector<HTMLButtonElement>('#save-game-screenplay')!;
  const stopRefreshing = () => { if (refreshTimer !== undefined) window.clearInterval(refreshTimer); refreshTimer = undefined; };
  const close = () => { stopRefreshing(); modal.remove(); };
  original.addEventListener('input', () => { originalDirty = true; });
  modal.querySelectorAll('.close,.game-screenplay-close').forEach(item => item.addEventListener('click', close));
  const renderState = (updated: Game) => {
    current = updated;
    const tasks = [...(updated.tasks || [])].reverse();
    const expansion = tasks.find(task => task.type === 'game_script_expansion' && task.status === '生成中');
    const graphPlanning = tasks.find(task => task.type === 'game_graph_decomposition' && task.status === '生成中');
    const busy = Boolean(expansion || graphPlanning);
    if (!originalDirty || busy) { original.value = updated.script; originalDirty = false; }
    const followsLatest = expanded.scrollTop + expanded.clientHeight >= expanded.scrollHeight - 24;
    if (expanded.value !== (updated.expanded_script || '')) {
      expanded.value = updated.expanded_script || '';
      if (followsLatest) expanded.scrollTop = expanded.scrollHeight;
    }
    original.disabled = busy;
    expanded.disabled = busy;
    saveButton.disabled = busy;
    expandButton.disabled = busy;
    regenerateButton.disabled = busy || regenerating;
    expandButton.textContent = busy ? '扩写中…' : expanded.value.trim() ? '继续扩写' : '从头扩写';
    cancelButton.hidden = !expansion;
    cancelButton.disabled = !expansion;
    if (expansion) {
      const length = (updated.expanded_script || '').length.toLocaleString();
      meta.textContent = `正在扩写互动游戏剧本，已保存 ${length} 字${expansion.stage ? `：${expansion.stage}` : '。'}`;
      if (refreshTimer === undefined) refreshTimer = window.setInterval(() => void loadGame(), 1_000);
    } else if (graphPlanning) {
      meta.textContent = '视频节点图谱正在拆分，完成后可继续编辑或再次扩写剧本。';
      stopRefreshing();
    } else {
      meta.textContent = '编辑原始剧本和分支剧本；保存不会自动修改现有视频节点图谱。重新生成会按原始剧本清空旧图谱并重新开始。';
      stopRefreshing();
    }
  };
  const loadGame = async () => {
    if (loading || !modal.isConnected) return;
    loading = true;
    try {
      const response = await fetch(`${options.apiBaseUrl}/games/${options.game.id}`);
      if (!response.ok) throw new Error(await responseError(response));
      if (modal.isConnected) renderState(await response.json() as Game);
    } catch (error) { console.error('互动游戏剧本加载失败', error); }
    finally { loading = false; }
  };
  renderState(current);
  void loadGame();
  expandButton.addEventListener('click', async () => {
    expandButton.disabled = true;
    expandButton.textContent = '启动中…';
    try {
      const response = await fetch(`${options.apiBaseUrl}/games/${options.game.id}/expanded-script/continue`, { method: 'POST' });
      if (!response.ok) throw new Error(await responseError(response));
      options.toast('已开始扩写互动游戏剧本');
      await loadGame();
      void options.refreshGame();
    } catch (error) { options.toast(`启动扩写失败：${error instanceof Error ? error.message : '请稍后重试'}`); console.error(error); }
    finally { if (modal.isConnected && !expandButton.disabled) expandButton.textContent = expanded.value.trim() ? '继续扩写' : '从头扩写'; }
  });
  cancelButton.addEventListener('click', async () => {
    cancelButton.disabled = true;
    cancelButton.textContent = '停止中…';
    try {
      const response = await fetch(`${options.apiBaseUrl}/games/${options.game.id}/expanded-script/cancel`, { method: 'POST' });
      if (!response.ok) throw new Error(await responseError(response));
      options.toast('剧本扩写已停止，已生成内容已保留');
      await loadGame();
      void options.refreshGame();
    } catch (error) { options.toast(`停止扩写失败：${error instanceof Error ? error.message : '请稍后重试'}`); console.error(error); }
    finally { if (modal.isConnected) cancelButton.textContent = '停止扩写'; }
  });
  regenerateButton.addEventListener('click', async () => {
    if (regenerating) return;
    const script = original.value.trim();
    if (script.length < 20) { options.toast('原始剧本不少于 20 个字'); original.focus(); return; }
    const confirmed = await confirmAction({
      title: '从头重新生成互动游戏？',
      description: '将清空当前扩写剧本、视频节点、选择、素材、视频历史和试玩记录，并取消正在进行的生成任务；随后按此处原始剧本重新开始。',
      confirmLabel: '重新生成',
    });
    if (!confirmed) return;
    regenerating = true;
    stopRefreshing();
    original.disabled = true;
    expanded.disabled = true;
    regenerateButton.disabled = true;
    regenerateButton.textContent = '重新生成中…';
    expandButton.disabled = true;
    cancelButton.hidden = true;
    saveButton.disabled = true;
    try {
      const response = await fetch(`${options.apiBaseUrl}/games/${options.game.id}/expanded-script/regenerate`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ script }),
      });
      if (!response.ok) throw new Error(await responseError(response));
      options.toast('已从原始剧本重新生成，正在展示生成进度');
      void options.refreshGame();
      close();
    } catch (error) {
      regenerating = false;
      options.toast(`重新生成失败：${error instanceof Error ? error.message : '请稍后重试'}`);
      console.error(error);
      renderState(current);
    }
  });
  saveButton.addEventListener('click', async () => {
    const script = original.value.trim();
    if (script.length < 20) { options.toast('原始剧本不少于 20 个字'); original.focus(); return; }
    saveButton.disabled = true;
    saveButton.textContent = '保存中…';
    try {
      const response = await fetch(`${options.apiBaseUrl}/games/${options.game.id}/expanded-script`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ script, expanded_script: expanded.value.trim() }) });
      if (!response.ok) throw new Error(await responseError(response));
      const updated = await response.json() as Game;
      options.replaceGame(updated);
      close();
      options.toast('剧本修改已保存；现有图谱保持不变');
      await options.refreshGame(updated);
    } catch (error) { saveButton.disabled = false; saveButton.textContent = '保存修改'; options.toast(`剧本保存失败：${error instanceof Error ? error.message : '请稍后重试'}`); console.error(error); }
  });
}
