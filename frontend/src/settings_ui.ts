/** Model, voice-catalog, and settings-card UI helpers. */

import type { Locale, ModelKind, ModelSettings, VoicePreset } from './models.js';

type SettingsRuntime = {
  apiBaseUrl: string;
  modelSettings: Record<ModelKind, ModelSettings>;
  defaultModelSettings: Record<ModelKind, ModelSettings>;
  getLocale: () => Locale;
  getVoicePresets: () => VoicePreset[];
  setVoicePresets: (items: VoicePreset[]) => void;
  getVoicePresetsLoaded: () => boolean;
  setVoicePresetsLoaded: (loaded: boolean) => void;
  isSettingsActive: () => boolean;
  render: () => void;
  escapeHtml: (value: unknown) => string;
  ui: (key: string) => string;
};

let runtime: SettingsRuntime;

export function configureSettingsRuntime(next: SettingsRuntime) {
  runtime = next;
}

export function modelChoices(kind: ModelKind) {
  const configured = runtime.modelSettings[kind].models;
  return Array.isArray(configured) ? configured : runtime.defaultModelSettings[kind].models;
}

export function modelEditorValues(card: HTMLElement) {
  return [...card.querySelectorAll<HTMLElement>('[data-model-entry]')]
    .map(item => item.dataset.modelEntry?.trim() || '')
    .filter((value, index, values) => value && values.indexOf(value) === index);
}

/** Returns the eye icon used by the API-key visibility toggle. */
export function apiKeyVisibilityIcon(revealed: boolean) {
  const slash = revealed ? '<path d="m3 3 18 18" />' : '';
  return `<svg class="model-api-key-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M2.1 12s3.2-5 9.9-5 9.9 5 9.9 5-3.2 5-9.9 5-9.9-5-9.9-5Z" /><circle cx="12" cy="12" r="2.2" />${slash}</svg>`;
}

export function renderModelEditor(card: HTMLElement, kind: ModelKind, values: string[], selected?: string) {
  const models = values.filter((value, index) => value && values.indexOf(value) === index);
  const trigger = card.querySelector<HTMLElement>('[data-model-trigger]');
  const options = card.querySelector<HTMLElement>('[data-model-options]');
  const menu = card.querySelector<HTMLElement>('[data-model-menu]');
  if (!trigger || !options || !menu) return;
  const current = selected || trigger.dataset.modelSelected || runtime.modelSettings[kind].model;
  const active = models.includes(current) ? current : (models[0] || '');
  trigger.dataset.modelSelected = active;
  trigger.setAttribute('aria-label', active ? `当前模型：${active}` : '选择模型');
  const label = trigger.querySelector<HTMLElement>('[data-model-selected-label]');
  if (label) label.textContent = active || '请选择模型';
  options.innerHTML = models.map(value => `<div class="model-choice-option ${value === active ? 'active' : ''}" data-model-entry="${runtime.escapeHtml(value)}" data-model-option="${runtime.escapeHtml(value)}"><button type="button" class="model-choice-option-select" data-model-select-option="${runtime.escapeHtml(value)}">${runtime.escapeHtml(value)}</button><button type="button" class="model-choice-option-delete" data-model-remove="${runtime.escapeHtml(value)}" aria-label="删除 ${runtime.escapeHtml(value)}" title="删除">🗑</button></div>`).join('') || '<div class="model-choice-empty">暂无模型，请在下方添加</div>';
  menu.hidden = true;
  trigger.setAttribute('aria-expanded', 'false');
}

/** Persist dropdown option edits without probing the provider connection. */
export async function saveModelOptions(kind: ModelKind, models: string[], model: string) {
  const response = await fetch(`${runtime.apiBaseUrl}/settings/models/${kind}/options`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ models, model }),
  });
  const payload = await response.json().catch(() => ({})) as ModelSettings & { detail?: string };
  if (!response.ok) throw new Error(payload.detail || `HTTP ${response.status}`);
  runtime.modelSettings[kind] = { ...runtime.modelSettings[kind], ...payload };
  refreshModelSelects();
  return payload;
}

function modelChoiceEditorMarkup(models: string[], selected: string) {
  const active = models.includes(selected) ? selected : (models[0] || '');
  return `<div class="model-choice-editor" data-model-list><div class="model-choice-label">可选模型名 <small>点击下方加号添加</small></div><button type="button" class="model-choice-trigger" data-model-trigger data-model-selected="${runtime.escapeHtml(active)}" aria-expanded="false" aria-label="当前模型：${runtime.escapeHtml(active)}"><span data-model-selected-label>${runtime.escapeHtml(active || '请选择模型')}</span><span class="model-choice-chevron">⌄</span></button><div class="model-choice-menu" data-model-menu hidden><div class="model-choice-options" data-model-options>${models.map(value => `<div class="model-choice-option ${value === active ? 'active' : ''}" data-model-entry="${runtime.escapeHtml(value)}" data-model-option="${runtime.escapeHtml(value)}"><button type="button" class="model-choice-option-select" data-model-select-option="${runtime.escapeHtml(value)}">${runtime.escapeHtml(value)}</button><button type="button" class="model-choice-option-delete" data-model-remove="${runtime.escapeHtml(value)}" aria-label="删除 ${runtime.escapeHtml(value)}" title="删除">🗑</button></div>`).join('') || '<div class="model-choice-empty">暂无模型，请在下方添加</div>'}</div><div class="model-choice-add"><input data-model-add placeholder="输入新的模型名称" /><button type="button" class="ghost" data-model-add-button aria-label="添加模型">＋</button></div></div></div>`;
}

export function applyModelSelect(root: ParentNode, selector: string, kind: ModelKind, selected?: string) {
  const select = root.querySelector<HTMLSelectElement>(selector);
  if (!select) return;
  const choices = modelChoices(kind);
  if (selected && !choices.includes(selected)) choices.unshift(selected);
  select.innerHTML = choices.map(value => `<option value="${runtime.escapeHtml(value)}">${runtime.escapeHtml(value)}</option>`).join('');
  select.value = selected && choices.includes(selected) ? selected : (runtime.modelSettings[kind].model || choices[0] || '');
}

export function refreshModelSelects() {
  applyModelSelect(document, '#configured-drama-language-model', 'language');
  applyModelSelect(document, '#configured-drama-image-model', 'multimodal');
  applyModelSelect(document, '#configured-drama-video-model', 'video');
  applyModelSelect(document, '#game-language-model', 'language');
  applyModelSelect(document, '#game-multimodal-model', 'multimodal');
}

export async function loadVoicePresets(force = false) {
  if (runtime.getVoicePresetsLoaded() && !force) return;
  try {
    const response = await fetch(`${runtime.apiBaseUrl}/settings/voices`);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const remote = await response.json() as VoicePreset[];
    runtime.setVoicePresets(Array.isArray(remote) ? remote : []);
    runtime.setVoicePresetsLoaded(true);
  } catch (error) {
    console.warn('音色列表加载失败', error);
  }
}

export function voicePreset(voiceId?: string | null) {
  return runtime.getVoicePresets().find(item => item.id === voiceId) || null;
}

export function voiceOptions(selected?: string | null) {
  return `<option value="">不设置</option>${runtime.getVoicePresets().filter(item => item.id !== 'none').map(item => `<option value="${runtime.escapeHtml(item.id)}" ${item.id === selected ? 'selected' : ''}>${runtime.escapeHtml(item.name)}</option>`).join('')}`;
}

export function voiceCatalogMarkup() {
  const presets = runtime.getVoicePresets();
  if (!presets.length) return '<p class="muted">音色列表加载中…</p>';
  return `<div class="voice-catalog-list">${presets.map(item => `<article class="voice-catalog-item"><div><strong>${runtime.escapeHtml(item.name)}</strong><small>${runtime.escapeHtml(item.gender || '未标注性别')}</small></div><p>${runtime.escapeHtml(item.prompt || '不绑定角色音色，沿用视频模型的默认声音表现。')}</p></article>`).join('')}</div>`;
}

/** Load the persisted provider choices before a project form picks its defaults. */
export async function loadModelSettings(): Promise<boolean> {
  try {
    const response = await fetch(`${runtime.apiBaseUrl}/settings/models`);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const remote = await response.json() as Partial<Record<ModelKind, ModelSettings>>;
    (['language', 'multimodal', 'video', 'audio'] as ModelKind[]).forEach(kind => {
      const item = remote[kind];
      if (!item) return;
      runtime.modelSettings[kind] = {
        ...runtime.defaultModelSettings[kind],
        ...item,
        models: Array.isArray(item.models) ? item.models : runtime.defaultModelSettings[kind].models,
      };
    });
    refreshModelSelects();
    if (runtime.isSettingsActive() && !document.querySelector('.modal-backdrop')) runtime.render();
    return true;
  } catch (error) {
    console.warn('模型配置加载失败，使用默认模型列表', error);
    return false;
  }
}

export function modelSettingsCard(kind: ModelKind, title: string, description: string, icon: string, saveLabel: string) {
  const config = runtime.modelSettings[kind];
  const models = modelChoices(kind);
  const provider = config.provider || 'ark';
  const providerSelect = `<label>${title}服务商<select data-model-provider><option value="ark" ${provider === 'ark' ? 'selected' : ''}>火山引擎</option><option value="dashscope" ${provider === 'dashscope' ? 'selected' : ''}>阿里云 DashScope</option><option value="tencent" ${provider === 'tencent' ? 'selected' : ''}>腾讯云</option></select></label>`;
  const endpoint = (label: string, fallback: string) => `<label>${label}<input data-model-endpoint value="${runtime.escapeHtml(config.endpoint || fallback)}" placeholder="${runtime.escapeHtml(fallback)}" /></label>`;
  const apiKey = (label = 'API Key') => `<label>${label}<div class="model-api-key-input"><input data-model-api-key type="password" autocomplete="new-password" placeholder="${config.api_key_set ? '********（已配置，点击眼睛查看）' : `请输入 ${label}`}" /><button type="button" class="ghost model-api-key-toggle" data-model-api-key-toggle aria-label="查看 ${label}" title="查看 ${label}">${apiKeyVisibilityIcon(false)}</button></div></label>`;
  const tencentSecrets = () => `<label>腾讯云地域<input data-model-region value="${runtime.escapeHtml(config.region || 'ap-guangzhou')}" placeholder="ap-guangzhou" /></label><label>腾讯云 SecretId<input data-model-secret-id autocomplete="off" placeholder="${runtime.escapeHtml(config.secret_id_masked ? `已配置 ${config.secret_id_masked}，留空保持不变` : '请输入 SecretId')}" /></label><label>腾讯云 SecretKey<input data-model-secret-key type="password" autocomplete="new-password" placeholder="${config.secret_key_set ? '已配置，留空保持不变' : '请输入 SecretKey'}" /></label>`;
  const connectionFields = (() => {
    if (kind === 'video') return provider === 'tencent' ? `${providerSelect}${endpoint('腾讯云 MPS Endpoint', 'https://mps.tencentcloudapi.com')}${tencentSecrets()}` : `${providerSelect}<label>${provider === 'dashscope' ? '阿里云创建视频任务 URL' : '创建视频生成任务 URL'}<input data-model-create-url value="${runtime.escapeHtml(config.create_url || '')}" placeholder="按服务商自动填充" /></label><label>${provider === 'dashscope' ? '阿里云查询视频任务 URL' : '查询视频生成任务 URL'}<input data-model-query-url value="${runtime.escapeHtml(config.query_url || '')}" placeholder="按服务商自动填充" /></label>${apiKey('API Key')}`;
    if (kind === 'audio') {
      if (provider === 'ark') return `${providerSelect}<label>火山引擎 TTS AppID<input data-model-app-id value="${runtime.escapeHtml(config.app_id || '')}" placeholder="语音应用 AppID" /></label><label>火山引擎提交任务 URL<input data-model-create-url value="${runtime.escapeHtml(config.create_url || '')}" placeholder="按服务商自动填充" /></label><label>火山引擎查询任务 URL<input data-model-query-url value="${runtime.escapeHtml(config.query_url || '')}" placeholder="按服务商自动填充" /></label><label>Resource-Id<input data-model-resource-id value="${runtime.escapeHtml(config.resource_id || 'volc.tts_async.default')}" placeholder="volc.tts_async.default" /></label><label>Voice Type<input data-model-voice value="${runtime.escapeHtml(config.voice || 'BV001_streaming')}" placeholder="BV001_streaming" /></label>${apiKey('火山引擎 Access Token')}`;
      if (provider === 'tencent') return `${providerSelect}${endpoint('腾讯云 MPS Endpoint', 'https://mps.tencentcloudapi.com')}${tencentSecrets()}<label>腾讯云 VoiceId<input data-model-voice value="${runtime.escapeHtml(config.voice || '')}" placeholder="MPS 可用 VoiceId" /></label>`;
      return `${providerSelect}${endpoint('阿里云 TTS Endpoint', 'https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation')}<label>阿里云 Voice<input data-model-voice value="${runtime.escapeHtml(config.voice || 'Cherry')}" placeholder="Cherry" /></label>${apiKey('API Key')}`;
    }
    const defaults = kind === 'language' ? { ark: 'https://ark.cn-beijing.volces.com/api/v3', dashscope: 'https://dashscope.aliyuncs.com/compatible-mode/v1', tencent: 'https://api.hunyuan.cloud.tencent.com/v1' } : { ark: 'https://ark.cn-beijing.volces.com/api/plan/v3', dashscope: 'https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation', tencent: 'https://tokenhub.tencentmaas.com/v1/api/image/submit' };
    return `${providerSelect}${endpoint(`${provider === 'tencent' ? '腾讯云' : provider === 'dashscope' ? '阿里云' : '火山引擎'} Endpoint`, defaults[provider])}${apiKey('API Key')}`;
  })();
  const queueLabel = ({ language: '剧本与提示词', multimodal: '图片', video: '视频', audio: '语音' } as Record<ModelKind, string>)[kind];
  const concurrencyField = `<label>${queueLabel}队列并发数<input data-generation-concurrency type="number" min="1" max="8" step="1" value="${runtime.escapeHtml(config.generation_concurrency || 2)}" /><small>此队列同时执行的任务数量；范围 1–8，与其他模型队列互不占用。</small></label>`;
  return `<div class="settings-card model-settings-card" data-model-config-card data-model-kind="${kind}"><div class="settings-card-header"><div class="setting-icon">${icon}</div><div><h2>${title}</h2><p>${description}</p></div></div>${connectionFields}${concurrencyField}${modelChoiceEditorMarkup(models, config.model)}<button class="primary wide" data-save-model-config>${saveLabel}</button></div>`;
}

export function voiceCatalogCard() {
  return `<section class="settings-card voice-catalog-settings-card" data-voice-catalog-card><div class="settings-card-header"><div class="setting-icon">♬</div><div><h2>系统音色库</h2><p>角色可从这里的音色集合中选择。音色描述会在分镜和视频提示词中保持一致。</p></div></div>${voiceCatalogMarkup()}</section>`;
}
