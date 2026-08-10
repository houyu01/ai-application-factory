import assert from 'node:assert/strict';
import test from 'node:test';

import { generationCopy } from '../src/drama_decomposition_banner_ui.ts';
import type { ApiProject, GenerationTask } from '../src/models.ts';

const project: ApiProject = {
  id: 'drama-1', name: '实时剧本', status: '生成中', ratio: '9:16', style: '真人风格', theme: '都市', created_at: '',
};

function task(type: string, progress: number, stage: string): GenerationTask {
  return { id: `${type}-${progress}`, type, status: '生成中', project_id: project.id, progress, stage };
}

test('expanding a bootstrap screenplay points creators to the live screenplay', () => {
  assert.equal(
    generationCopy(project, task('script_decomposition', 59, '正在扩写第011至第015集')).title,
    '扩写剧本(点击上方“剧本”查看实时剧本)',
  );
});

test('continuing a screenplay keeps the same live-screenplay guidance', () => {
  assert.equal(
    generationCopy(project, task('script_expansion', 60, '正在继续扩写剧本')).title,
    '扩写剧本(点击上方“剧本”查看实时剧本)',
  );
});

test('storyboard decomposition retains its existing step title', () => {
  assert.equal(
    generationCopy(project, task('script_decomposition', 75, '正在整理分集、分镜和素材')).title,
    '第 3/4 步：拆解分镜',
  );
});
