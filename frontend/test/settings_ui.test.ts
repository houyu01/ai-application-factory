import assert from 'node:assert/strict';
import test from 'node:test';

import { configureSettingsRuntime, configuredModelSelection, hasEnteredApiKey, isCurrentModelSettingsResponse, modelSettingsCard, restoreSettingsScroll, voiceCatalogMarkup, voicePreviewCanEdit, voicePreviewStyle } from '../src/settings_ui.ts';
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

test('Volcengine audio profile uses the Seed-TTS 2.0 Agent Plan HTTP endpoint', () => {
  const profile = providerModelProfile('audio', 'ark');

  assert.equal(profile.endpoint, 'https://openspeech.bytedance.com/api/v3/plan/tts/unidirectional');
  assert.equal(profile.resource_id, undefined);
  assert.equal(profile.voice, undefined);
  assert.deepEqual(profile.models, ['seed-tts-2.0']);
});

test('Volcengine audio card exposes the default model as an editable choice', () => {
  const audio = {
    kind: 'audio' as const,
    provider: 'ark' as const,
    endpoint: 'https://openspeech.bytedance.com/api/v3/plan/tts/unidirectional',
    model: 'seed-tts-2.0',
    models: ['seed-tts-2.0'],
  };
  configureSettingsRuntime({
    apiBaseUrl: '',
    modelSettings: { audio } as Record<ModelKind, ModelSettings>,
    defaultModelSettings: { audio } as Record<ModelKind, ModelSettings>,
    getLocale: () => 'zh',
    getVoicePresets: () => [],
    setVoicePresets: () => undefined,
    getVoicePresetsLoaded: () => true,
    setVoicePresetsLoaded: () => undefined,
    isSettingsActive: () => true,
    render: () => undefined,
    escapeHtml: value => String(value),
    resolveMediaUrl: value => value || '',
    ui: key => key,
  });

  const markup = modelSettingsCard('audio', '音频模型', '配音', '♫', '保存');

  assert.match(markup, /豆包语音合成模型 2\.0 HTTP URL/);
  assert.match(markup, /豆包语音 API Key/);
  assert.match(markup, /可选模型名/);
  assert.match(markup, /seed-tts-2\.0/);
  assert.doesNotMatch(markup, /AppID|Resource-Id|Speaker|提交任务 URL|查询任务 URL/);
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

test('a newly entered API key can be revealed before it is saved', () => {
  assert.equal(hasEnteredApiKey('  sk-entered-locally  '), true);
  assert.equal(hasEnteredApiKey('   '), false);
});

test('voice catalog includes a form for creator-defined names and descriptions', () => {
  const presets: VoicePreset[] = [
    { id: 'cold_boss_male', name: '冷酷霸总音（男）', gender: '男', prompt: '低沉克制的声线。', audio_url: '/api/media/system-voice-cold_boss_male.mp3' },
    { id: 'custom-voice', name: '客户专属旁白', gender: '女', prompt: '清晰温和的叙述声线。' },
  ];
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
    resolveMediaUrl: value => value || '',
    ui: key => key,
  });

  const markup = voiceCatalogMarkup();

  assert.match(markup, /客户专属旁白/);
  assert.match(markup, /data-voice-preset-form/);
  assert.match(markup, /音色描述/);
  assert.doesNotMatch(markup, /data-voice-audio-generate="cold_boss_male"/);
  assert.match(markup, /data-voice-audio-generate="custom-voice"/);
});

test('custom voice preview declares the title, gender, and description sent to the model', () => {
  assert.equal(
    voicePreviewStyle({ name: '冷酷霸总音（男）', gender: '男', prompt: '成年男性低沉有磁性的声线。' }),
    '标题：冷酷霸总音（男）；性别：男；描述：成年男性低沉有磁性的声线。',
  );
});

test('only failed custom voice previews expose the edit action', () => {
  assert.equal(voicePreviewCanEdit('生成失败'), true);
  assert.equal(voicePreviewCanEdit('生成中'), false);
  assert.equal(voicePreviewCanEdit('已完成'), false);
});
