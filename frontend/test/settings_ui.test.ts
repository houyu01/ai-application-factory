import assert from 'node:assert/strict';
import test from 'node:test';

import { configureSettingsRuntime, configuredModelSelection, isCurrentModelSettingsResponse, restoreSettingsScroll, voiceCatalogMarkup } from '../src/settings_ui.ts';
import type { ModelKind, ModelSettings, VoicePreset } from '../src/models.ts';
import { providerModelProfile } from '../src/model_provider_profiles.ts';

test('project video selector drops a stale model after the provider changes', () => {
  const alibabaModels = ['wan2.6-r2v-flash'];

  assert.equal(
    configuredModelSelection(alibabaModels, 'doubao-seedance-2.0', 'wan2.6-r2v-flash'),
    'wan2.6-r2v-flash',
  );
  assert.deepEqual(alibabaModels, ['wan2.6-r2v-flash']);
});

test('project video selector retains a model configured for the active provider', () => {
  assert.equal(configuredModelSelection(['wan2.6-r2v-flash', 'wan2.7-r2v'], 'wan2.7-r2v', 'wan2.6-r2v-flash'), 'wan2.7-r2v');
});

test('Tencent image profile uses the MPS TC3 endpoint and model notation', () => {
  const profile = providerModelProfile('multimodal', 'tencent');

  assert.equal(profile.endpoint, 'https://mps.tencentcloudapi.com');
  assert.equal(profile.model, 'Hunyuan:3.0');
  assert.deepEqual(profile.models, ['Hunyuan:3.0']);
});

test('DashScope video profile defaults to a reference-to-video model', () => {
  const profile = providerModelProfile('video', 'dashscope');

  assert.equal(profile.model, 'wan2.6-r2v-flash');
  assert.deepEqual(profile.models, ['wan2.6-r2v-flash', 'wan2.6-r2v', 'wan2.7-r2v']);
});

test('settings rerender restores the current main-pane scroll position', () => {
  const pane = { scrollTop: 0 };

  restoreSettingsScroll(true, 684, pane);

  assert.equal(pane.scrollTop, 684);
});

test('non-settings rerenders leave the new main pane at its default position', () => {
  const pane = { scrollTop: 0 };

  restoreSettingsScroll(false, 684, pane);

  assert.equal(pane.scrollTop, 0);
});

test('an older model-settings response cannot replace a newer model list', () => {
  assert.equal(isCurrentModelSettingsResponse(3, 4), false);
  assert.equal(isCurrentModelSettingsResponse(4, 4), true);
});

test('voice catalog includes a form for creator-defined names and descriptions', () => {
  const presets: VoicePreset[] = [{ id: 'custom-voice', name: '客户专属旁白', gender: '女', prompt: '清晰温和的叙述声线。' }];
  configureSettingsRuntime({
    apiBaseUrl: '',
    modelSettings: {} as Record<ModelKind, ModelSettings>,
    defaultModelSettings: {} as Record<ModelKind, ModelSettings>,
    getLocale: () => 'zh',
    getVoicePresets: () => presets,
    setVoicePresets: () => undefined,
    getVoicePresetsLoaded: () => true,
    setVoicePresetsLoaded: () => undefined,
    isSettingsActive: () => true,
    render: () => undefined,
    escapeHtml: value => String(value),
    ui: key => key,
  });

  const markup = voiceCatalogMarkup();

  assert.match(markup, /客户专属旁白/);
  assert.match(markup, /data-voice-preset-form/);
  assert.match(markup, /音色描述/);
});
