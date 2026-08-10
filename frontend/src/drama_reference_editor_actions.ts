import { dramaPromptNodes, dramaSelectedShot, readDramaPromptNodes, renderDramaPromptNodes, serializeDramaPromptNodes, setupDramaRichPromptEditor, syncDramaShotReferencePanel } from './drama_core_ui.js';
import { openDramaReferenceMentionPicker, openDramaReferencePicker } from './drama_reference_picker.js';
import { reconcileDramaReferenceNodes } from './drama_reference_picker_selection.js';
import { dramaReferenceAssetIds } from './drama_reference_removal.js';
import { activeDramaProject, dramaViewState, setActiveDramaProject } from './drama_state.js';
import type { ApiProject, DramaPromptNode } from './models.js';

type Runtime = { apiBaseUrl: string; toast: (message: string) => void };

let runtime: Runtime | null = null;

function openCurrentShotReferencePicker() {
  const projectId = dramaViewState.projectId;
  if (!runtime || !projectId) return;
  void fetch(`${runtime.apiBaseUrl}/projects/${projectId}`).then(response => {
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    return response.json() as Promise<ApiProject>;
  }).then(project => {
    const shot = dramaSelectedShot(project);
    if (!shot) throw new Error('当前没有可编辑的分镜');
    if (!document.querySelector('.drama-rich-prompt-editor')) setupDramaRichPromptEditor(project, shot);
    const editor = document.querySelector<HTMLElement>('.drama-rich-prompt-editor');
    const promptInput = document.querySelector<HTMLTextAreaElement>('#drama-shot-prompt');
    const existingNodes: DramaPromptNode[] = editor
      ? readDramaPromptNodes(editor)
      : dramaPromptNodes(project, shot);
    openDramaReferencePicker(project, existingNodes, nodes => {
      const currentEditor = document.querySelector<HTMLElement>('.drama-rich-prompt-editor');
      if (currentEditor) {
        renderDramaPromptNodes(currentEditor, project, reconcileDramaReferenceNodes(readDramaPromptNodes(currentEditor), nodes));
        currentEditor.dispatchEvent(new Event('input', { bubbles: true }));
        const currentShot = dramaSelectedShot(project);
        if (currentShot) currentShot.prompt_rich = readDramaPromptNodes(currentEditor);
      } else if (promptInput) {
        const serialized = serializeDramaPromptNodes(project, reconcileDramaReferenceNodes(existingNodes, nodes));
        promptInput.value = serialized.prompt;
        promptInput.dataset.promptRich = JSON.stringify(serialized.nodes);
        shot.prompt_rich = serialized.nodes;
      }
      setActiveDramaProject(project);
      syncDramaShotReferencePanel(project);
      runtime?.toast('参考图选择已更新，请使用右上角保存');
    });
  }).catch(error => { runtime?.toast('参考图加载失败'); console.error(error); });
}

function insertDramaMentionAtSelection(editor: HTMLElement, project: ApiProject) {
  const selection = window.getSelection();
  const selectedRange = selection?.rangeCount && selection.getRangeAt(0);
  const range = selectedRange && editor.contains(selectedRange.startContainer) && editor.contains(selectedRange.endContainer)
    ? selectedRange.cloneRange()
    : (() => { const fallback = document.createRange(); fallback.selectNodeContents(editor); fallback.collapse(false); return fallback; })();
  openDramaReferenceMentionPicker(project, node => {
    if (!editor.isConnected) return;
    editor.focus();
    range.deleteContents();
    const temporary = document.createElement('span');
    renderDramaPromptNodes(temporary, project, [node]);
    const chip = temporary.firstElementChild;
    if (!chip) return;
    range.insertNode(chip);
    const spacer = document.createTextNode(' ');
    range.setStartAfter(chip);
    range.collapse(true);
    range.insertNode(spacer);
    range.setStartAfter(spacer);
    range.collapse(true);
    const activeSelection = window.getSelection();
    activeSelection?.removeAllRanges();
    activeSelection?.addRange(range);
    editor.dispatchEvent(new Event('input', { bubbles: true }));
  });
}

/** Binds the picker button and @-mention insertion for the current rich prompt editor. */
export function configureDramaReferenceEditorActions(value: Runtime) {
  runtime = value;
  document.addEventListener('click', event => {
    const target = event.target instanceof HTMLElement ? event.target : null;
    if (!target?.closest('[data-drama-add-reference]')) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    openCurrentShotReferencePicker();
  }, true);
  document.addEventListener('beforeinput', event => {
    const input = event as InputEvent;
    const target = input.target instanceof HTMLElement ? input.target.closest<HTMLElement>('.drama-rich-prompt-editor') : null;
    if (input.data !== '@' || !target || !activeDramaProject) return;
    event.preventDefault();
    insertDramaMentionAtSelection(target, activeDramaProject);
  }, true);
  document.addEventListener('input', event => {
    const editor = event.target instanceof HTMLElement ? event.target.closest<HTMLElement>('.drama-rich-prompt-editor') : null;
    if (!editor || !activeDramaProject) return;
    const shot = dramaSelectedShot(activeDramaProject);
    if (!shot) return;
    const serialized = serializeDramaPromptNodes(activeDramaProject, readDramaPromptNodes(editor));
    shot.prompt = serialized.prompt;
    shot.prompt_rich = serialized.nodes;
    shot.reference_asset_ids = dramaReferenceAssetIds(serialized.nodes);
    syncDramaShotReferencePanel(activeDramaProject);
  });
}
