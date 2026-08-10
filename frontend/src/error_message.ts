/** Convert raw model-provider failures into short Chinese copy before they reach a toast. */
export function translateErrorMessage(message: string) {
  const source = message.trim();
  const code = errorCode(source);
  const translated = translationFor(code, source);
  if (!translated) return source;
  const prefix = source.match(/^(.{0,80}?失败[：:])/u)?.[1];
  return prefix ? `${prefix}${translated}` : translated;
}

/** Identify toast content that deserves the longer reading time reserved for failures. */
export function isErrorMessage(message: string) {
  return /失败|错误|异常|error|failed|http\s*\d{3}|timeout|unauthorized|forbidden/i.test(message);
}

export function toastDuration(message: string) {
  return isErrorMessage(message) ? 8_000 : 2_600;
}

function errorCode(message: string) {
  const match = message.match(/(?:"code"\s*:\s*"|错误码[：:]\s*)([\w.-]+)/i);
  return match?.[1]?.toLowerCase() || '';
}

function translationFor(code: string, message: string) {
  const source = message.toLowerCase();
  if (code.includes('inputimagesensitivecontentdetected') || code.includes('privacyinformation') || source.includes('real person') || source.includes('privacy information')) return '检测到输入图片可能包含真人或个人隐私信息，服务商拒绝生成。请替换为不含真人或隐私信息的图片后重试。';
  if (code.includes('unsupportedmodel') || source.includes('does not support') || source.includes('unsupported model')) return '当前模型不支持此功能，请在设置中更换支持该功能的模型后重试。';
  if (code.includes('invalidapikey') || code.includes('authentication') || source.includes('invalid api key') || source.includes('unauthorized') || /http\s*401/.test(source)) return 'API Key 无效或已失效，请检查模型配置中的 API Key。';
  if (code.includes('permission') || code.includes('forbidden') || source.includes('permission denied') || source.includes('forbidden') || /http\s*403/.test(source)) return '当前账号没有调用此模型的权限，请检查模型开通状态和 API Key 权限。';
  if (code.includes('ratelimit') || code.includes('quota') || source.includes('rate limit') || source.includes('too many requests') || /http\s*429/.test(source)) return '请求过于频繁或账户额度不足，请稍后重试并检查账户额度。';
  if (code.includes('sensitive') || code.includes('contentpolicy') || source.includes('sensitive content') || source.includes('content policy')) return '输入内容未通过安全审核，请修改提示词或参考素材后重试。';
  if (code.includes('invalidparameter') || code.includes('badrequest') || source.includes('invalid parameter') || source.includes('bad request') || /http\s*400/.test(source)) return '请求参数不符合服务商要求，请检查模型、提示词和参考素材后重试。';
  if (code.includes('notfound') || source.includes('not found') || /http\s*404/.test(source)) return '所选模型或服务地址不存在，请检查 Endpoint 和模型名称。';
  if (source.includes('timeout') || source.includes('timed out') || /http\s*408/.test(source)) return '服务商响应超时，请稍后重试。';
  if (source.includes('failed to fetch') || source.includes('network error') || source.includes('connection')) return '无法连接到服务商，请检查网络和服务地址后重试。';
  if (/http\s*5\d{2}|service unavailable|internal server error/.test(source)) return '服务商暂时不可用，请稍后重试。';
  return isEnglishOnlyError(source) ? '服务商未能完成请求，请检查模型配置、提示词和参考素材后重试。' : '';
}

function isEnglishOnlyError(message: string) {
  return /error|failed|request|response|provider|exception/i.test(message) && !/[\u4e00-\u9fff]/u.test(message);
}
