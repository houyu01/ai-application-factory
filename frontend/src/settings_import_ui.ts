/** Settings header actions for all-or-nothing local JSON and invite-code imports. */

type ImportRuntime = {
  apiBaseUrl: string;
  toast: (message: string) => void;
  reload: () => Promise<unknown>;
};

let runtime: ImportRuntime;

export function configureSettingsImportRuntime(next: ImportRuntime) {
  runtime = next;
}

export function inviteCodeError(value: string) {
  if (!value) return '请输入邀请码';
  return /^[A-Za-z0-9]{6}$/.test(value) ? '' : '邀请码必须是 6 位字母或数字';
}

export function settingsImportActionsMarkup() {
  return `<div class="settings-import-actions"><button type="button" class="ghost" data-settings-local-import>⇧ 本地导入</button><button type="button" class="primary" data-settings-invite-import>邀请码配置</button><input type="file" accept=".json,application/json" data-settings-import-file hidden /></div>`;
}

async function importPayload(payload: Record<string, unknown>, button: HTMLButtonElement, modal?: HTMLElement) {
  const idle = button.textContent || '导入';
  button.disabled = true;
  button.classList.add('is-loading');
  button.innerHTML = '<span class="generation-spinner" aria-hidden="true"></span><span>校验并嗅探 5 项配置…</span>';
  try {
    const response = await fetch(`${runtime.apiBaseUrl}/settings/import`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    const result = await response.json().catch(() => ({})) as { detail?: string };
    if (!response.ok) throw new Error(result.detail || `HTTP ${response.status}`);
    modal?.remove();
    await runtime.reload();
    runtime.toast('5 项配置均已校验、嗅探并保存');
  } catch (error) {
    runtime.toast(`配置导入失败，未保存任何更改：${error instanceof Error ? error.message : '请检查配置'}`);
  } finally {
    if (button.isConnected) {
      button.disabled = false;
      button.classList.remove('is-loading');
      button.textContent = idle;
    }
  }
}

function openInviteModal() {
  const backdrop = document.createElement('div');
  backdrop.className = 'modal-backdrop settings-invite-backdrop';
  backdrop.innerHTML = `<section class="modal settings-invite-modal" role="dialog" aria-modal="true" aria-labelledby="settings-invite-title"><button type="button" class="close" data-invite-close aria-label="关闭">×</button><div class="modal-head"><h2 id="settings-invite-title">邀请码配置</h2><p>输入 6 位邀请码，将从安全的固定配置地址下载，并统一校验、嗅探和保存 5 项配置。</p></div><label>邀请码<input data-invite-code maxlength="6" inputmode="text" autocomplete="off" spellcheck="false" placeholder="例如 A1B2C3" aria-describedby="settings-invite-error" /></label><p class="settings-invite-error" id="settings-invite-error" aria-live="polite"></p><div class="modal-actions"><button type="button" class="ghost" data-invite-close>取消</button><button type="button" class="primary" data-invite-submit disabled>导入</button></div></section>`;
  document.body.append(backdrop);
  const input = backdrop.querySelector<HTMLInputElement>('[data-invite-code]')!;
  const submit = backdrop.querySelector<HTMLButtonElement>('[data-invite-submit]')!;
  const error = backdrop.querySelector<HTMLElement>('.settings-invite-error')!;
  const validate = () => {
    input.value = input.value.replace(/\s/g, '').slice(0, 6);
    const message = inviteCodeError(input.value);
    error.textContent = input.value ? message : '';
    submit.disabled = Boolean(message);
  };
  input.addEventListener('input', validate);
  input.addEventListener('keydown', event => { if (event.key === 'Enter' && !submit.disabled) submit.click(); });
  backdrop.querySelectorAll<HTMLElement>('[data-invite-close]').forEach(button => button.addEventListener('click', () => backdrop.remove()));
  submit.addEventListener('click', () => void importPayload({ invite_code: input.value }, submit, backdrop));
  input.focus();
}

if (typeof document !== 'undefined') document.addEventListener('click', event => {
  const target = event.target instanceof Element ? event.target : null;
  if (target?.closest('[data-settings-invite-import]')) {
    openInviteModal();
    return;
  }
  if (!target?.closest('[data-settings-local-import]')) return;
  document.querySelector<HTMLInputElement>('[data-settings-import-file]')?.click();
});

if (typeof document !== 'undefined') document.addEventListener('change', event => {
  const input = event.target instanceof HTMLInputElement && event.target.matches('[data-settings-import-file]') ? event.target : null;
  const file = input?.files?.[0];
  if (!input || !file) return;
  const button = document.querySelector<HTMLButtonElement>('[data-settings-local-import]');
  if (!button) return;
  void file.text()
    .then(text => {
      let config: unknown;
      try { config = JSON.parse(text); } catch { throw new Error('所选文件不是有效 JSON'); }
      return importPayload({ config }, button);
    })
    .catch(error => runtime.toast(`本地配置读取失败：${error instanceof Error ? error.message : '请重新选择文件'}`))
    .finally(() => { input.value = ''; });
});
