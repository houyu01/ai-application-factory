import assert from 'node:assert/strict';
import test from 'node:test';
import { changedProjectPayload, changedShotPayload, type DramaEditorSnapshot } from '../src/drama_editor_autosave.ts';

const draft = (overrides: Partial<DramaEditorSnapshot> = {}): DramaEditorSnapshot => ({
  projectId: 'project-1', shotId: 'shot-1', projectName: '短剧', title: '分镜 1',
  originalText: '原文', prompt: '提示词', promptRich: [{ type: 'text', text: '提示词' }], durationSeconds: 10,
  ...overrides,
});

test('autosave sends no request when the editor snapshot is unchanged', () => {
  const saved = draft();
  assert.equal(changedProjectPayload(draft(), saved), null);
  assert.equal(changedShotPayload(draft(), saved), null);
});

test('autosave keeps a prompt edit isolated from unchanged shot fields', () => {
  const saved = draft();
  assert.deepEqual(changedShotPayload(draft({ prompt: '更新提示词', promptRich: [{ type: 'text', text: '更新提示词' }] }), saved), {
    prompt: '更新提示词', prompt_rich: [{ type: 'text', text: '更新提示词' }],
  });
});

test('autosave skips an empty project title while retaining shot changes', () => {
  const saved = draft();
  const edited = draft({ projectName: ' ', originalText: '更新原文' });
  assert.equal(changedProjectPayload(edited, saved), null);
  assert.deepEqual(changedShotPayload(edited, saved), { original_text: '更新原文' });
});
