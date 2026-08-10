/** Expose a direct retry action after restart recovery marks local-only work as failed. */
import type { ApiProject, GenerationTask } from './models.js';

function latestTask(project: ApiProject, type: string, resourceId?: string) {
  return [...(project.tasks || [])].reverse().find(task => task.type === type
    && (resourceId === undefined || task.resource_id === resourceId));
}

function retryTask(project: ApiProject, type: string, resourceId?: string) {
  const task = latestTask(project, type, resourceId);
  return task?.status === '生成失败' ? task : undefined;
}

function restartLabel(button: HTMLButtonElement | null, task: GenerationTask | undefined, label: string) {
  if (!button || !task) return;
  button.innerHTML = `↻ ${label}`;
  button.title = task.error_message?.trim() || `${label}失败，请重试。`;
}

/** Keep failed local work actionable after the project is reopened. */
export function syncDramaTaskRetryControls(project: ApiProject, shotId?: string) {
  document.querySelectorAll<HTMLButtonElement>('[data-drama-generate-asset]').forEach(button => {
    restartLabel(button, retryTask(project, 'asset_image', button.dataset.dramaGenerateAsset), '重试生成图片');
  });
  document.querySelectorAll<HTMLButtonElement>('[data-drama-generate-variant]').forEach(button => {
    restartLabel(button, retryTask(project, 'asset_variant_image', button.dataset.dramaVariantId), '重试生成图片');
  });
  if (!shotId) return;
  restartLabel(
    document.querySelector<HTMLButtonElement>('#drama-generate-shot-prompt'),
    retryTask(project, 'shot_prompt', shotId),
    '重试生成提示词',
  );
  restartLabel(
    document.querySelector<HTMLButtonElement>('[data-drama-quality-check]'),
    retryTask(project, 'shot_quality', shotId),
    '重新运行检查',
  );
}
