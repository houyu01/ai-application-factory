import type { ApiProject, GenerationTask } from './models.js';

type DramaTaskPollingRuntime = {
  apiBaseUrl: string;
  getProject: () => ApiProject | null;
  onStatusUpdate: (tasks: GenerationTask[], completed: GenerationTask[]) => Promise<void> | void;
};

type TaskStatusResponse = {
  server_time?: string;
  tasks?: GenerationTask[];
};

let runtime: DramaTaskPollingRuntime | null = null;
let timer: number | null = null;
let polling = false;
let projectId: string | null = null;
let cursor = '';

export function configureDramaTaskPolling(value: DramaTaskPollingRuntime) {
  runtime = value;
}

export function stopDramaTaskPolling() {
  if (timer !== null) {
    window.clearTimeout(timer);
    timer = null;
  }
}

export function scheduleDramaTaskRefresh(project: ApiProject) {
  if (!runtime) return;
  if (projectId !== project.id) {
    stopDramaTaskPolling();
    projectId = project.id;
    cursor = '';
  }
  const hasActiveTask = (project.tasks || []).some(task => task.status === '生成中');
  if (!hasActiveTask) {
    stopDramaTaskPolling();
    return;
  }
  if (timer === null && !polling) {
    timer = window.setTimeout(() => {
      timer = null;
      void pollDramaTaskStatuses(project.id);
    }, 1000);
  }
}

async function pollDramaTaskStatuses(id: string) {
  if (!runtime || polling) return;
  const current = runtime.getProject();
  if (!current || current.id !== id) {
    stopDramaTaskPolling();
    return;
  }
  polling = true;
  try {
    const query = new URLSearchParams({ status: '生成中' });
    if (cursor) query.set('since', cursor);
    const response = await fetch(
      `${runtime.apiBaseUrl}/projects/${encodeURIComponent(id)}/tasks?${query.toString()}`,
    );
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const payload = await response.json() as TaskStatusResponse;
    cursor = payload.server_time || cursor;
    const tasks = payload.tasks || [];
    const completed = tasks.filter(task => task.status !== '生成中');
    await runtime.onStatusUpdate(tasks, completed);
  } catch (error) {
    console.warn('短剧任务状态轮询失败', error);
    if (runtime.getProject()?.id === id) {
      timer = window.setTimeout(() => {
        timer = null;
        void pollDramaTaskStatuses(id);
      }, 3000);
    }
  } finally {
    polling = false;
    const latest = runtime?.getProject();
    if (latest?.id === id && (latest.tasks || []).some(task => task.status === '生成中')) {
      scheduleDramaTaskRefresh(latest);
    } else {
      stopDramaTaskPolling();
    }
  }
}
