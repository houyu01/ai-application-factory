import { isErrorMessage, toastDuration, translateErrorMessage } from './error_message.js';

/** Show one dismissible notification; failures remain visible long enough to read. */
export function toast(message: string) {
  const isError = isErrorMessage(message);
  const notice = document.createElement('div');
  notice.className = `toast${isError ? ' toast-error' : ''}`;
  notice.setAttribute('role', isError ? 'alert' : 'status');
  const icon = document.createElement('span');
  icon.className = 'toast-icon';
  icon.textContent = isError ? '!' : '✓';
  const content = document.createElement('span');
  content.className = 'toast-message';
  content.textContent = translateErrorMessage(message);
  const close = document.createElement('button');
  close.className = 'toast-close';
  close.type = 'button';
  close.setAttribute('aria-label', '关闭提示');
  close.textContent = '×';
  notice.append(icon, content, close);
  document.body.append(notice);
  let timeout: number | undefined;
  const dismiss = () => { if (timeout !== undefined) window.clearTimeout(timeout); notice.remove(); };
  close.addEventListener('click', dismiss);
  timeout = window.setTimeout(dismiss, toastDuration(message));
}
