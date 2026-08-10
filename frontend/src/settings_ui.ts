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
let modelSettingsLoadVersion = 0;

function modelCompatibilityHint(kind: ModelKind, provider: NonNullable<ModelSettings['provider']>, model: string) {
  if (kind === 'multimodal' && provider === 'tencent') return '腾讯云 MPS 使用 SecretId / SecretKey；当前 Hunyuan:3.0 按文生图提交，生成结果会立即转存到本地。';
  if (kind === 'multimodal' && provider === 'dashscope') return 'Qwen-Image 2.0 系列按 messages 输入；选择编辑模型时可附带 1–3 张参考图。';
  if (kind === 'video' && provider === 'dashscope') return `${model || 'Wan R2V'} 通过 reference_urls 接收参考素材；R2V 模型必须至少有一张参考图，Wan 2.6 中国区请填写工作空间 Endpoint。`;
  if (kind === 'video' && provider === 'tencent') return '腾讯云 MPS 使用 TC3 签名；参考图片必须是外网可访问 URL，返回的视频地址会立即转存。';
  if (kind === 'audio' && provider === 'dashscope') return 'Qwen3-TTS-Flash 直接返回音频 URL；Instruct 版本支持额外的语气指令。';
  if (kind === 'audio' && provider === 'tencent') return '腾讯云使用 SyncDubbing，同步返回音频 URL 或 Base64；VoiceId 必填。';
  return '保存时会按所选服务商和模型执行一次真实连通性嗅探，成功后才替换当前配置。';
}

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

/** Pick a project model only from the active provider's configured model list. */
export function configuredModelSelection(models: string[], selected?: string, defaultModel?: string) {
  if (selected && models.includes(selected)) return selected;
  if (defaultModel && models.includes(defaultModel)) return defaultModel;
  return models[0] || '';
}

/** Ignore an older model-settings response when a newer reload is already pending. */
export function isCurrentModelSettingsResponse(responseVersion: number, latestVersion: number) {
  return responseVersion === latestVersion;
}

/** Restore the settings pane after a settings-only rerender replaces its scroll container. */
export function restoreSettingsScroll(isSettingsActive: boolean, scrollTop: number | undefined, target?: { scrollTop: number } | null) {
  if (isSettingsActive && scrollTop !== undefined && target) target.scrollTop = scrollTop;
}

/** Rerender settings content without resetting the scrollable desktop main pane. */
export function rerenderSettingsPreservingScroll() {
  const isSettingsActive = runtime.isSettingsActive();
  const scrollTop = isSettingsActive ? document.querySelector<HTMLElement>('.shell > main')?.scrollTop : undefined;
  runtime.render();
  restoreSettingsScroll(isSettingsActive, scrollTop, document.querySelector<HTMLElement>('.shell > main'));
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
    body: JSON.stringify({ models, model, provider: runtime.modelSettings[kind].provider || 'ark' }),
  });
  const payload = await response.json().catch(() => ({})) as ModelSettings & { detail?: string };
  if (!response.ok) throw new Error(payload.detail || `HTTP ${response.status}`);
  runtime.modelSettings[kind] = { ...runtime.modelSettings[kind], ...payload };
  refreshModelSelects();
  return payload;
}

function modelChoiceEditorMarkup(models: string[], selected: string) {
  const active = models.includes(selected) ? selected : (models[0] || '');
  return `<div class="model-choice-editor" data-model-list><div class="model-choice-label">可选模型名 <small>添加后需点击“保存配置”生效</small></div><button type="button" class="model-choice-trigger" data-model-trigger data-model-selected="${runtime.escapeHtml(active)}" aria-expanded="false" aria-label="当前模型：${runtime.escapeHtml(active)}"><span data-model-selected-label>${runtime.escapeHtml(active || '请选择模型')}</span><span class="model-choice-chevron">⌄</span></button><div class="model-choice-menu" data-model-menu hidden><div class="model-choice-options" data-model-options>${models.map(value => `<div class="model-choice-option ${value === active ? 'active' : ''}" data-model-entry="${runtime.escapeHtml(value)}" data-model-option="${runtime.escapeHtml(value)}"><button type="button" class="model-choice-option-select" data-model-select-option="${runtime.escapeHtml(value)}">${runtime.escapeHtml(value)}</button><button type="button" class="model-choice-option-delete" data-model-remove="${runtime.escapeHtml(value)}" aria-label="删除 ${runtime.escapeHtml(value)}" title="删除">🗑</button></div>`).join('') || '<div class="model-choice-empty">暂无模型，请在下方添加</div>'}</div><div class="model-choice-add"><input data-model-add placeholder="输入新的模型名称" /><button type="button" class="ghost" data-model-add-button aria-label="添加模型">＋</button></div></div></div>`;
}

export function applyModelSelect(root: ParentNode, selector: string, kind: ModelKind, selected?: string) {
  const select = root.querySelector<HTMLSelectElement>(selector);
  if (!select) return;
  const choices = [...modelChoices(kind)];
  const active = configuredModelSelection(choices, selected, runtime.modelSettings[kind].model);
  select.innerHTML = choices.map(value => `<option value="${runtime.escapeHtml(value)}">${runtime.escapeHtml(value)}</option>`).join('');
  select.value = active;
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
    if (runtime.isSettingsActive() && !document.querySelector('.modal-backdrop')) rerenderSettingsPreservingScroll();
  } catch (error) {
    console.warn('音色列表加载失败', error);
  }
}

/** Persist one creator-defined voice, then refresh the settings catalog and every shared selector source. */
export async function createVoicePreset(values: Pick<VoicePreset, 'name' | 'gender' | 'prompt'>) {
  const response = await fetch(`${runtime.apiBaseUrl}/settings/voices`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(values),
  });
  const payload = await response.json().catch(() => ({})) as VoicePreset & { detail?: string };
  if (!response.ok) throw new Error(payload.detail || `HTTP ${response.status}`);
  runtime.setVoicePresets([...runtime.getVoicePresets(), payload]);
  runtime.setVoicePresetsLoaded(true);
  rerenderSettingsPreservingScroll();
  return payload;
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
  return `<div class="voice-catalog-list">${presets.map(item => `<article class="voice-catalog-item"><div><strong>${runtime.escapeHtml(item.name)}</strong><small>${runtime.escapeHtml(item.gender || '未标注性别')}</small></div><p>${runtime.escapeHtml(item.prompt || '不绑定角色音色，沿用视频模型的默认声音表现。')}</p></article>`).join('')}${voicePresetFormMarkup()}</div>`;
}

function voicePresetFormMarkup() {
  return `<article class="voice-catalog-add-card"><div class="voice-catalog-add-heading"><strong>＋ 追加自定义音色</strong><small>保存后即可为角色选择</small></div><form data-voice-preset-form><label>音色名称<input name="name" maxlength="80" required placeholder="例如：知性旁白女声" /></label><label>适用性别<select name="gender"><option value="">未标注</option><option value="男">男</option><option value="女">女</option><option value="中性">中性</option></select></label><label class="voice-catalog-description">音色描述<textarea name="prompt" maxlength="500" rows="3" required placeholder="描述声线、语速、情绪与表达特点…"></textarea></label><button class="primary" type="submit">追加音色</button></form></article>`;
}

/** Load the persisted provider choices before a project form picks its defaults. */
export async function loadModelSettings(): Promise<boolean> {
  const responseVersion = ++modelSettingsLoadVersion;
  try {
    const response = await fetch(`${runtime.apiBaseUrl}/settings/models`);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const remote = await response.json() as Partial<Record<ModelKind, ModelSettings>>;
    if (!isCurrentModelSettingsResponse(responseVersion, modelSettingsLoadVersion)) return true;
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
    if (runtime.isSettingsActive() && !document.querySelector('.modal-backdrop')) rerenderSettingsPreservingScroll();
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
    if (kind === 'multimodal' && provider === 'tencent') return `${providerSelect}${endpoint('腾讯云 MPS Endpoint', 'https://mps.tencentcloudapi.com')}${tencentSecrets()}`;
    const defaults = kind === 'language' ? { ark: 'https://ark.cn-beijing.volces.com/api/v3', dashscope: 'https://dashscope.aliyuncs.com/compatible-mode/v1', tencent: 'https://api.hunyuan.cloud.tencent.com/v1' } : { ark: 'https://ark.cn-beijing.volces.com/api/plan/v3', dashscope: 'https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation', tencent: 'https://tokenhub.tencentmaas.com/v1/api/image/submit' };
    return `${providerSelect}${endpoint(`${provider === 'tencent' ? '腾讯云' : provider === 'dashscope' ? '阿里云' : '火山引擎'} Endpoint`, defaults[provider])}${apiKey('API Key')}`;
  })();
  const queueLabel = ({ language: '剧本与提示词', multimodal: '图片', video: '视频', audio: '语音' } as Record<ModelKind, string>)[kind];
  const concurrencyField = `<label>${queueLabel}队列并发数<input data-generation-concurrency type="number" min="1" max="8" step="1" value="${runtime.escapeHtml(config.generation_concurrency || 2)}" /><small>此队列同时执行的任务数量；范围 1–8，与其他模型队列互不占用。</small></label>`;
  const hint = modelCompatibilityHint(kind, provider, config.model);
  return `<div class="settings-card model-settings-card" data-model-config-card data-model-kind="${kind}"><div class="settings-card-header"><div class="setting-icon">${icon}</div><div><h2>${title}</h2><p>${description}</p></div></div>${connectionFields}<p class="muted model-compatibility-hint">${runtime.escapeHtml(hint)}</p>${concurrencyField}${modelChoiceEditorMarkup(models, config.model)}<button class="primary wide" data-save-model-config>${saveLabel}</button></div>`;
}

export function voiceCatalogCard() {
  return `<section class="settings-card voice-catalog-settings-card" data-voice-catalog-card><div class="settings-card-header"><div class="setting-icon">♬</div><div><h2>系统音色库</h2><p>角色可从这里的音色集合中选择。音色描述会在分镜和视频提示词中保持一致。</p></div></div>${voiceCatalogMarkup()}</section>`;
}
