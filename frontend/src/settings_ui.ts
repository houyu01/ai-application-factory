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
  const configured = runtime.modelSettings[kind].models || [];
  return configured.length ? configured : runtime.defaultModelSettings[kind].models;
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
        models: Array.isArray(item.models) && item.models.length ? item.models : runtime.defaultModelSettings[kind].models,
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
  const endpointFields = kind === 'video' ? `<label>创建视频生成任务 URL<input data-model-create-url value="${runtime.escapeHtml(config.create_url || '')}" placeholder="https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks" /></label><label>查询视频生成任务 URL<input data-model-query-url value="${runtime.escapeHtml(config.query_url || '')}" placeholder="https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks/{id}" /></label>` : `<label>Endpoint<input data-model-endpoint value="${runtime.escapeHtml(config.endpoint || '')}" placeholder="https://ark.cn-beijing.volces.com/api/plan/v3" /></label>`;
  return `<div class="settings-card model-settings-card" data-model-config-card data-model-kind="${kind}"><div class="settings-card-header"><div class="setting-icon">${icon}</div><div><h2>${title}</h2><p>${description}</p></div></div>${endpointFields}<label>API Key<div class="model-api-key-input"><input data-model-api-key type="password" autocomplete="new-password" placeholder="${config.api_key_set ? '********（已配置，点击眼睛查看）' : '请输入 API Key'}" /><button type="button" class="ghost model-api-key-toggle" data-model-api-key-toggle aria-label="查看 API Key" title="查看 API Key">${apiKeyVisibilityIcon(false)}</button></div></label>${modelChoiceEditorMarkup(models, config.model)}<button class="primary wide" data-save-model-config>${saveLabel}</button></div>`;
}

export function voiceCatalogCard() {
  return `<section class="settings-card voice-catalog-settings-card" data-voice-catalog-card><div class="settings-card-header"><div class="setting-icon">♬</div><div><h2>系统音色库</h2><p>角色可从这里的音色集合中选择。音色描述会在分镜和视频提示词中保持一致。</p></div></div>${voiceCatalogMarkup()}</section>`;
}
