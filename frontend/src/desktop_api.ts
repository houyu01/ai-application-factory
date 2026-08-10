import { invoke } from '@tauri-apps/api/core';

type DesktopResponse = {
  status: number;
  body: unknown;
  content_type?: string;
};

const DESKTOP_API_BASE = 'tauri://ai-application-factory/api';
const nativeFetch = window.fetch.bind(window);
const isDesktop = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

function isDesktopApi(url: URL) {
  return url.protocol === 'tauri:' && url.hostname === 'ai-application-factory' && url.pathname.startsWith('/api');
}

function requestUrl(input: RequestInfo | URL) {
  if (input instanceof Request) return new URL(input.url);
  return new URL(String(input), window.location.href);
}

async function requestBody(input: RequestInfo | URL, init?: RequestInit) {
  if (typeof init?.body === 'string') return init.body;
  if (input instanceof Request && !init?.body) return input.clone().text();
  return undefined;
}

async function desktopFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const url = requestUrl(input);
  if (!isDesktop || !isDesktopApi(url)) return nativeFetch(input, init);
  const inheritedMethod = input instanceof Request ? input.method : 'GET';
  const response = await invoke<DesktopResponse>('api_request', {
    request: {
      method: init?.method || inheritedMethod,
      path: `${url.pathname.slice('/api'.length)}${url.search}` || '/',
      body: await requestBody(input, init),
    },
  });
  return new Response(response.body === null ? null : JSON.stringify(response.body), {
    status: response.status,
    headers: { 'content-type': response.content_type || 'application/json; charset=utf-8' },
  });
}

/** Return the local in-process API base used by the desktop application. */
export function apiBaseUrl() {
  return DESKTOP_API_BASE;
}

/** Convert persisted local media references into the custom Tauri media protocol. */
export function resolveDesktopMediaUrl(value: string) {
  if (!isDesktop) return value;
  const match = value.match(/(?:tauri:\/\/ai-application-factory)?\/api\/media\/([^/?#]+)/);
  return match ? `media://localhost/${encodeURIComponent(match[1])}` : value;
}

window.fetch = desktopFetch;
