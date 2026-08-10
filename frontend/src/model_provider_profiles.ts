/** Provider-native defaults and compatibility guidance for the media workbench. */

import type { ModelKind, ModelProvider, ModelSettings } from './models.js';

type Provider = ModelProvider;

const profiles: Record<ModelKind, Record<Provider, Partial<ModelSettings>>> = {
  language: {
    ark: { endpoint: 'https://ark.cn-beijing.volces.com/api/v3', model: 'doubao-seed-2.1-turbo', models: ['doubao-seed-2.1-turbo'] },
    dashscope: { endpoint: 'https://dashscope.aliyuncs.com/compatible-mode/v1', model: 'qwen-plus', models: ['qwen-plus'] },
    tencent: { endpoint: 'https://api.hunyuan.cloud.tencent.com/v1', model: 'hunyuan-turbos-latest', models: ['hunyuan-turbos-latest'] },
  },
  multimodal: {
    ark: { endpoint: 'https://ark.cn-beijing.volces.com/api/plan/v3', model: 'doubao-seedream-4-0-250828', models: ['doubao-seedream-4-0-250828'] },
    dashscope: { endpoint: 'https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation', model: 'qwen-image-2.0', models: ['qwen-image-2.0', 'qwen-image-2.0-pro'] },
    tencent: { endpoint: 'https://mps.tencentcloudapi.com', region: 'ap-guangzhou', model: 'Hunyuan:3.0', models: ['Hunyuan:3.0'] },
  },
  video: {
    ark: { create_url: 'https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks', query_url: 'https://ark.cn-beijing.volces.com/api/plan/v3/contents/generations/tasks/{id}', endpoint: '', model: 'doubao-seedance-2.0', models: ['doubao-seedance-2.0'] },
    dashscope: { create_url: 'https://dashscope.aliyuncs.com/api/v1/services/aigc/video-generation/video-synthesis', query_url: 'https://dashscope.aliyuncs.com/api/v1/tasks/{id}', endpoint: '', model: 'wan2.6-r2v-flash', models: ['wan2.6-r2v-flash', 'wan2.6-r2v', 'wan2.7-r2v'] },
    tencent: { endpoint: 'https://mps.tencentcloudapi.com', create_url: 'https://mps.tencentcloudapi.com', query_url: 'https://mps.tencentcloudapi.com', region: 'ap-guangzhou', model: 'Hunyuan:1.5', models: ['Hunyuan:1.5'] },
  },
  audio: {
    ark: { endpoint: '', create_url: 'https://openspeech.bytedance.com/api/v1/tts_async/submit', query_url: 'https://openspeech.bytedance.com/api/v1/tts_async/query', resource_id: 'volc.tts_async.default', voice: 'BV001_streaming', model: 'volc.tts_async.default', models: ['volc.tts_async.default'] },
    dashscope: { endpoint: 'https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation', model: 'qwen3-tts-flash', voice: 'Cherry', models: ['qwen3-tts-flash', 'qwen3-tts-instruct-flash'] },
    tencent: { endpoint: 'https://mps.tencentcloudapi.com', region: 'ap-guangzhou', model: 'mps-sync-dubbing', voice: '', models: ['mps-sync-dubbing'] },
  },
};

export function providerModelProfile(kind: ModelKind, provider: Provider) {
  return profiles[kind][provider] || profiles[kind].ark;
}

/** Merge a saved public provider profile over the native defaults without leaking another provider's key state. */
export function restoredProviderModelProfile(current: ModelSettings, kind: ModelKind, provider: Provider): ModelSettings {
  const defaults = providerModelProfile(kind, provider);
  const saved = current.provider_profiles?.[provider];
  const model = saved?.model || defaults.model || '';
  const models = saved?.models?.length ? saved.models : (defaults.models?.length ? defaults.models : [model]);
  return {
    endpoint: '',
    generation_concurrency: 2,
    ...defaults,
    ...saved,
    kind,
    provider,
    model,
    models,
    provider_profiles: current.provider_profiles,
  };
}
