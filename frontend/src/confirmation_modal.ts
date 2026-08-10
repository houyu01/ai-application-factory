/** Reusable in-app confirmation dialog for actions that cannot rely on WebView browser prompts. */

export type ConfirmationOptions = {
  title: string;
  description: string;
  confirmLabel?: string;
  cancelLabel?: string;
};

/** Opens a modal confirmation dialog and resolves after the user explicitly chooses an action. */
export function confirmAction(options: ConfirmationOptions): Promise<boolean> {
  return new Promise(resolve => {
    const backdrop = document.createElement('div');
    backdrop.className = 'modal-backdrop app-confirm-backdrop';
    backdrop.innerHTML = '<section class="modal app-confirm-modal" role="alertdialog" aria-modal="true" aria-labelledby="app-confirm-title" aria-describedby="app-confirm-description"><h2 id="app-confirm-title"></h2><p id="app-confirm-description"></p><div class="app-confirm-actions"><button type="button" class="ghost" data-confirm-cancel></button><button type="button" class="danger-button" data-confirm-accept></button></div></section>';
    const title = backdrop.querySelector<HTMLElement>('#app-confirm-title')!;
    const description = backdrop.querySelector<HTMLElement>('#app-confirm-description')!;
    const cancel = backdrop.querySelector<HTMLButtonElement>('[data-confirm-cancel]')!;
    const accept = backdrop.querySelector<HTMLButtonElement>('[data-confirm-accept]')!;
    title.textContent = options.title;
    description.textContent = options.description;
    cancel.textContent = options.cancelLabel || '取消';
    accept.textContent = options.confirmLabel || '确认删除';
    const finish = (confirmed: boolean) => {
      document.removeEventListener('keydown', onKeydown);
      backdrop.remove();
      resolve(confirmed);
    };
    const onKeydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') finish(false);
    };
    cancel.addEventListener('click', () => finish(false));
    accept.addEventListener('click', () => finish(true));
    backdrop.addEventListener('click', event => { if (event.target === backdrop) finish(false); });
    document.addEventListener('keydown', onKeydown);
    document.body.append(backdrop);
    cancel.focus();
  });
}
