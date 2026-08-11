/** Settings events for generating, retrying, and confirming source-audio voice previews. */

import { confirmVoiceAudioPreview, createVoiceAudioPreview, editVoiceAudioPreview, regenerateVoiceAudioPreview, voicePreset } from './settings_ui.js';

export function configureVoiceCatalogInteractions(toast: (message: string) => void) {
  document.addEventListener('submit', event => {
    const form = event.target instanceof HTMLFormElement ? event.target : null;
    if (!form?.matches('[data-voice-preset-form]')) return;
    event.preventDefault();
    const name = (form.elements.namedItem('name') as HTMLInputElement | null)?.value.trim() || '';
    const gender = (form.elements.namedItem('gender') as HTMLSelectElement | null)?.value || '';
    const prompt = (form.elements.namedItem('prompt') as HTMLTextAreaElement | null)?.value.trim() || '';
    if (!name || !prompt) { toast('请填写音色名称和音色描述'); return; }
    const button = form.querySelector<HTMLButtonElement>('button[type="submit"]');
    if (button) button.disabled = true;
    void createVoiceAudioPreview({ name, gender, prompt }).then(() => {
      toast('已创建音色试听，请生成完成后播放确认');
    }).catch(error => {
      toast(`音色试听创建失败：${error instanceof Error ? error.message : '请重试'}`);
      if (button) button.disabled = false;
    });
  }, true);
  document.addEventListener('click', event => {
    const target = event.target instanceof Element ? event.target : null;
    const edit = target?.closest<HTMLButtonElement>('[data-voice-preview-edit]');
    const regenerate = target?.closest<HTMLButtonElement>('[data-voice-preview-regenerate]');
    const confirm = target?.closest<HTMLButtonElement>('[data-voice-preview-confirm]');
    const catalogGenerate = target?.closest<HTMLButtonElement>('[data-voice-audio-generate]');
    const action = edit || regenerate || confirm || catalogGenerate;
    if (!action) return;
    event.preventDefault();
    if (edit) {
      if (editVoiceAudioPreview(edit.dataset.voicePreviewEdit || '')) toast('已带回表单，可修改后重新生成试听');
      return;
    }
    const idle = action.textContent || '操作'; action.disabled = true;
    const catalogVoice = catalogGenerate ? voicePreset(catalogGenerate.dataset.voiceAudioGenerate) : null;
    const request = catalogVoice
      ? createVoiceAudioPreview({ name: catalogVoice.name, gender: catalogVoice.gender, prompt: catalogVoice.prompt, voice_id: catalogVoice.id })
      : regenerate ? regenerateVoiceAudioPreview(regenerate.dataset.voicePreviewRegenerate || '')
        : confirmVoiceAudioPreview(confirm!.dataset.voicePreviewConfirm || '');
    void request.then(result => {
      if (confirm && result && 'name' in result) toast(`已追加音色：${result.name}`);
      else toast('已创建新的音色试听任务');
    }).catch(error => {
      toast(`音色操作失败：${error instanceof Error ? error.message : '请重试'}`);
      action.disabled = false; action.textContent = idle;
    });
  }, true);
}
