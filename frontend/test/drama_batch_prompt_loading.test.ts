import assert from 'node:assert/strict';
import test from 'node:test';

import { batchPromptLoadingState, shouldUpdateTaskControl } from '../src/drama_batch_prompt_loading.ts';
import type { ApiProject } from '../src/models.ts';

function projectWithTasks(tasks: ApiProject['tasks']): ApiProject {
  return { id: 'project-1', name: '测试短剧', status: '生成成功', ratio: '9:16', style: '', theme: '', created_at: '', shots: [{ id: 'shot-1', title: '', original_text: '', prompt: '', status: '未生成' }], tasks };
}

test('placeholder image task never activates the batch prompt loading state', () => {
  const project = projectWithTasks([{ id: 'placeholder-task', type: 'placeholder_image', status: '生成中', project_id: 'project-1', resource_id: 'placeholder-1', input_snapshot: { shot_id: 'shot-1' } }]);

  assert.deepEqual(batchPromptLoadingState(project), { loading: false, queuedCount: 0 });
  assert.equal(shouldUpdateTaskControl(new Set(['placeholder_image']), 'shot_prompt'), false);
});

test('only active shot prompt tasks activate the batch prompt loading state', () => {
  const project = projectWithTasks([{ id: 'prompt-task', type: 'shot_prompt', status: '生成中', project_id: 'project-1', resource_id: 'shot-1', stage: '正在执行' }]);

  assert.deepEqual(batchPromptLoadingState(project), { loading: true, queuedCount: 0 });
  assert.equal(shouldUpdateTaskControl(new Set(['shot_prompt']), 'shot_prompt'), true);
});

test('a stale prompt task does not put the batch control into loading', () => {
  const project = projectWithTasks([{ id: 'stale-prompt-task', type: 'shot_prompt', status: '生成中', project_id: 'project-1', resource_id: 'deleted-shot', stage: '正在执行' }]);

  assert.deepEqual(batchPromptLoadingState(project), { loading: false, queuedCount: 0 });
});
