/** Modal for viewing the stored long-form screenplay without loading it into the editor. */

type ExpandedScriptDialogOptions = {
  apiBaseUrl: string;
  projectId: string;
  toast: (message: string) => void;
};

type ExpandedScriptResponse = {
  script?: string;
  expanded_script?: string;
  expanded_script_preview?: string;
  expanded_script_generating?: boolean;
  expanded_script_cancellable?: boolean;
  expanded_script_cancel_label?: string;
  expanded_script_task_status?: string;
  expanded_script_error_message?: string | null;
  expanded_script_stage?: string | null;
  expanded_script_length?: number;
  original_script_length?: number;
};

type ExpandedScriptCancellationResponse = {
  status?: string;
};

function scriptMeta(payload: ExpandedScriptResponse, original: string, expanded: string) {
  const originalLength = payload.original_script_length ?? original.length;
  const expandedLength = payload.expanded_script_length ?? expanded.length;
  const errorMessage = payload.expanded_script_error_message?.trim();
  const expandedMeta = errorMessage
    ? `扩写失败，任务已停止：${errorMessage}`
    : payload.expanded_script_task_status === '生成失败'
    ? '扩写失败，任务已停止。'
    : payload.expanded_script_task_status === '已取消'
    ? `扩写已取消，已保留 ${expandedLength.toLocaleString()} 字。`
    : payload.expanded_script_generating
    ? `扩写后剧本正在生成中，已输出 ${expandedLength.toLocaleString()} 字。`
    : `扩写后剧本 ${expandedLength.toLocaleString()} 字。`;
  return `原始剧本 ${originalLength.toLocaleString()} 字；${expandedMeta}`;
}

/** Fetch, edit, and save both screenplay versions through the dedicated endpoint. */
export function openDramaExpandedScriptModal(options: ExpandedScriptDialogOptions) {
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop';
  modal.innerHTML = `<div class="modal drama-expanded-script-modal" role="dialog" aria-modal="true" aria-labelledby="expanded-script-title"><button class="close" aria-label="关闭">×</button><div class="modal-head"><h2 id="expanded-script-title">剧本</h2><p data-expanded-script-meta>正在加载剧本…</p></div><div class="drama-expanded-script-fields"><label><span>原始剧本</span><textarea data-original-script rows="9" disabled></textarea></label><label><span>扩写后剧本</span><textarea data-expanded-script rows="16" disabled></textarea></label></div><div class="video-prompt-actions drama-expanded-script-actions"><div class="drama-expanded-script-action-group"><button class="ghost" data-expanded-script-continue hidden title="基于当前扩写剧本继续调用模型">继续扩写</button><button class="ghost" data-expanded-script-cancel hidden>取消扩写</button></div><div class="drama-expanded-script-action-group"><button class="ghost" data-expanded-script-close>关闭</button><button class="primary" data-expanded-script-save disabled>保存修改并分镜</button></div></div></div>`;
  document.body.append(modal);
  let refreshTimer: number | undefined;
  let loading = false;
  let loadSequence = 0;
  let cancelling = false;
  const stopRefreshing = () => {
    if (refreshTimer !== undefined) window.clearInterval(refreshTimer);
    refreshTimer = undefined;
  };
  const close = () => { stopRefreshing(); modal.remove(); };
  modal.querySelectorAll<HTMLElement>('.close,[data-expanded-script-close]').forEach(button => button.addEventListener('click', close));
  const originalInput = modal.querySelector<HTMLTextAreaElement>('[data-original-script]')!;
  const expandedInput = modal.querySelector<HTMLTextAreaElement>('[data-expanded-script]')!;
  const meta = modal.querySelector<HTMLElement>('[data-expanded-script-meta]')!;
  const cancelButton = modal.querySelector<HTMLButtonElement>('[data-expanded-script-cancel]')!;
  const continueButton = modal.querySelector<HTMLButtonElement>('[data-expanded-script-continue]')!;
  const saveButton = modal.querySelector<HTMLButtonElement>('[data-expanded-script-save]')!;

  const loadScreenplay = async (force = false) => {
    if (cancelling && !force) return;
    if (loading && !force) return;
    const sequence = ++loadSequence;
    loading = true;
    try {
      const response = await fetch(`${options.apiBaseUrl}/projects/${encodeURIComponent(options.projectId)}/expanded-script`);
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const payload = await response.json() as ExpandedScriptResponse;
      if (!modal.isConnected || sequence !== loadSequence) return;
      const expandedGenerating = payload.expanded_script_generating === true;
      const expansionCancellable = expandedGenerating && payload.expanded_script_cancellable !== false;
      const expandedFailed = payload.expanded_script_task_status === '生成失败';
      // Keep already-rendered stream text visible if a failed-task response
      // arrives without a persisted checkpoint. The API normally supplies the
      // latest failed-task preview, while this protects the transition itself.
      const streamedScript = payload.expanded_script_preview || payload.expanded_script || expandedInput.value;
      const followsLatest = expandedInput.scrollTop + expandedInput.clientHeight >= expandedInput.scrollHeight - 24;
      originalInput.value = payload.script || '';
      if (expandedInput.value !== streamedScript) {
        expandedInput.value = streamedScript;
        if (followsLatest) expandedInput.scrollTop = expandedInput.scrollHeight;
      }
      expandedInput.placeholder = expandedGenerating ? '正在生成中，内容会实时显示在这里…' : '';
      expandedInput.setAttribute('aria-busy', String(expandedGenerating));
      meta.textContent = scriptMeta(payload, originalInput.value, streamedScript);
      if (expandedFailed && payload.expanded_script_stage && !payload.expanded_script_error_message) {
        meta.textContent += `（${payload.expanded_script_stage}）`;
      }
      originalInput.disabled = false;
      expandedInput.disabled = expandedGenerating;
      saveButton.disabled = expandedGenerating;
      cancelButton.hidden = !expansionCancellable;
      cancelButton.disabled = !expansionCancellable || cancelling;
      if (!cancelling) cancelButton.textContent = payload.expanded_script_cancel_label || '取消扩写';
      const hasExpandableScript = streamedScript.trim().length > 0;
      continueButton.hidden = false;
      continueButton.disabled = expandedGenerating || !hasExpandableScript;
      if (expandedGenerating && refreshTimer === undefined) refreshTimer = window.setInterval(() => void loadScreenplay(), 1_000);
      if (!expandedGenerating) stopRefreshing();
    } catch (error) {
      if (sequence !== loadSequence) return;
      meta.textContent = '加载剧本失败，请稍后重试。';
      options.toast('扩写剧本加载失败');
      console.error(error);
    } finally {
      if (sequence === loadSequence) loading = false;
    }
  };
  void loadScreenplay();

  cancelButton.addEventListener('click', async () => {
    if (cancelling) return;
    cancelling = true;
    const cancellationLabel = cancelButton.textContent?.trim() || '取消扩写';
    // A prior polling response can otherwise repaint the dialog as
    // ``generating`` after this click, making a successful cancellation look
    // like it was ignored. Invalidate it and pause all subsequent polls.
    loadSequence += 1;
    stopRefreshing();
    cancelButton.disabled = true;
    cancelButton.textContent = '取消中…';
    meta.textContent = `正在${cancellationLabel}，等待后台确认任务已停止…`;
    try {
      const response = await fetch(`${options.apiBaseUrl}/projects/${encodeURIComponent(options.projectId)}/expanded-script/cancel`, {
        method: 'POST',
      });
      if (!response.ok) {
        const detail = await response.json().catch(() => ({}));
        throw new Error(detail.detail || `HTTP ${response.status}`);
      }
      const task = await response.json() as ExpandedScriptCancellationResponse;
      if (task.status === '生成失败') {
        options.toast('扩写任务已因失败停止，无需重复取消');
      } else if (task.status === '已取消') {
        options.toast(`${cancellationLabel}已确认，后台任务已停止`);
      } else {
        options.toast('扩写任务状态已确认');
      }
      cancelling = false;
      await loadScreenplay(true);
    } catch (error) {
      options.toast(error instanceof Error ? error.message : '取消扩写失败');
      console.error(error);
      cancelling = false;
      await loadScreenplay(true);
    } finally {
      cancelling = false;
    }
  });

  continueButton.addEventListener('click', async () => {
    let queued = false;
    continueButton.disabled = true;
    continueButton.textContent = '扩写中…';
    try {
      const response = await fetch(`${options.apiBaseUrl}/projects/${encodeURIComponent(options.projectId)}/expanded-script/continue`, {
        method: 'POST',
      });
      if (!response.ok) {
        const detail = await response.json().catch(() => ({}));
        throw new Error(detail.detail || `HTTP ${response.status}`);
      }
      queued = true;
      options.toast('已基于当前剧本开始继续扩写');
      await loadScreenplay();
    } catch (error) {
      options.toast(error instanceof Error ? error.message : '继续扩写失败');
      console.error(error);
    } finally {
      continueButton.textContent = '继续扩写';
      if (!queued) continueButton.disabled = false;
    }
  });

  saveButton.addEventListener('click', async () => {
    const script = originalInput.value.trim();
    const expandedScript = expandedInput.value.trim();
    if (script.length < 10) {
      options.toast('原始剧本不少于 10 个字');
      originalInput.focus();
      return;
    }
    saveButton.disabled = true;
    saveButton.textContent = '保存中…';
    try {
      const response = await fetch(`${options.apiBaseUrl}/projects/${encodeURIComponent(options.projectId)}/expanded-script`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ script, expanded_script: expandedScript }),
      });
      if (!response.ok) {
        const detail = await response.json().catch(() => ({}));
        throw new Error(detail.detail || `HTTP ${response.status}`);
      }
      const saved = await response.json() as ExpandedScriptResponse;
      originalInput.value = saved.script || script;
      expandedInput.value = saved.expanded_script || expandedScript;
      meta.textContent = scriptMeta(saved, originalInput.value, expandedInput.value);
      options.toast('剧本修改已保存；现有分镜保持不变');
    } catch (error) {
      options.toast(error instanceof Error ? error.message : '剧本保存失败');
      console.error(error);
    } finally {
      saveButton.disabled = false;
      saveButton.textContent = '保存修改并分镜';
    }
  });
}
