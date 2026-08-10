import type { ApiProject } from './models.js';

/** Derives the top-level prompt control state from prompt tasks only. */
export function batchPromptLoadingState(project: ApiProject) {
  const shotIds = new Set((project.shots || []).map(shot => shot.id));
  const activePrompts = (project.tasks || []).filter(task => (
    task.type === 'shot_prompt'
    && task.status === '生成中'
    && Boolean(task.resource_id && shotIds.has(task.resource_id))
  ));
  return {
    loading: activePrompts.some(task => task.stage !== '等待队列'),
    queuedCount: activePrompts.filter(task => task.stage === '等待队列').length,
  };
}

/** Limits a partial task refresh to the controls owned by the changed task type. */
export function shouldUpdateTaskControl(updatedTypes: ReadonlySet<string> | undefined, taskType: string) {
  return !updatedTypes || updatedTypes.has(taskType);
}
