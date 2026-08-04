/** Collapsible quality-check UI for locating issues in the drama shot editor. */
import type { DramaShot, GenerationTask } from './models.js';
import { icon } from './ui_icons.js';

type QualityRenderInput = {
  promptPanel: HTMLElement;
  shot: DramaShot;
  qualityTask?: GenerationTask;
  escapeHtml: (value: unknown) => string;
  taskProgressLabel: (task?: GenerationTask) => string;
};

function qualityTarget(field?: string) {
  if (['references', 'scene_reference_ids'].includes(field || '')) return '.drama-reference-panel';
  if (['original_text', 'shot_text'].includes(field || '')) return '#drama-shot-original';
  if (['duration', 'duration_seconds', 'shot_constraints'].includes(field || '')) return '.drama-params';
  return '.drama-rich-prompt-frame';
}

function focusQualityIssue(field?: string) {
  const target = document.querySelector<HTMLElement>(qualityTarget(field));
  if (!target) return;
  target.scrollIntoView({ behavior: 'smooth', block: 'center' });
  target.classList.remove('quality-focus-flash');
  window.setTimeout(() => target.classList.add('quality-focus-flash'), 80);
  window.setTimeout(() => target.classList.remove('quality-focus-flash'), 1550);
}

/** Render a compact quality summary after the prompt so warnings do not dominate the editor. */
export function renderDramaQualityPanel(input: QualityRenderInput) {
  const quality = input.promptPanel.querySelector<HTMLElement>('[data-drama-quality-panel]') || document.createElement('section');
  const expanded = quality.classList.contains('is-expanded');
  const issues = input.shot.quality_issues || [];
  const pending = Boolean(input.qualityTask && !['生成成功', '生成失败'].includes(input.qualityTask.status));
  const status = pending ? '检查中' : input.shot.quality_status || '未检查';
  const hasWarning = issues.length > 0 || ['需修改', '检查失败'].includes(status);
  const score = input.shot.quality?.score;
  const headline = pending
    ? `正在自动检查${input.taskProgressLabel(input.qualityTask)}`
    : hasWarning ? `${issues.length || 1} 项需要处理` : status === '通过' ? '检查通过' : status;
  const details = pending
    ? `<p class="drama-quality-pending">正在自动检查分镜提示词，请稍候。</p>`
    : issues.length
      ? `<div class="drama-quality-issues">${issues.map(issue => `<button type="button" class="drama-quality-issue ${issue.severity === 'error' ? 'is-error' : ''}" data-drama-quality-focus="${input.escapeHtml(issue.field || 'prompt')}">${icon('warning')}<span>${input.escapeHtml(issue.message || issue.code || '需要检查')}</span></button>`).join('')}</div>`
      : `<p class="drama-quality-pending">${status === '通过' ? '提示词结构、参考素材和项目约束检查通过。' : status === '检查失败' ? '自动检查失败，请重新生成分镜提示词。' : '生成提示词后会自动检查。'}</p>`;
  quality.className = `drama-quality-panel ${hasWarning ? 'has-warning' : ''}`;
  quality.dataset.dramaQualityPanel = 'true';
  quality.innerHTML = `<button type="button" class="drama-quality-toggle" data-drama-quality-toggle aria-expanded="${expanded}"><span class="drama-quality-marker ${hasWarning ? 'has-warning' : ''}">${hasWarning ? icon('warning') : pending ? '⟳' : '✓'}</span><span class="drama-quality-title">分镜质量检查</span><span class="drama-quality-state">${input.escapeHtml(headline)}${score !== undefined ? ` · ${input.escapeHtml(String(score))} 分` : ''}</span><span class="drama-quality-chevron" aria-hidden="true">⌄</span></button><div class="drama-quality-details" data-drama-quality-details ${expanded ? '' : 'hidden'}>${details}</div>`;
  quality.classList.toggle('is-expanded', expanded);
  if (!quality.parentElement) input.promptPanel.append(quality);
}

document.addEventListener('click', event => {
  const target = event.target instanceof Element ? event.target : null;
  const toggle = target?.closest<HTMLButtonElement>('[data-drama-quality-toggle]');
  const issue = target?.closest<HTMLButtonElement>('[data-drama-quality-focus]');
  if (toggle) {
    event.preventDefault();
    const panel = toggle.closest<HTMLElement>('[data-drama-quality-panel]');
    const details = panel?.querySelector<HTMLElement>('[data-drama-quality-details]');
    if (!panel || !details) return;
    const expanded = toggle.getAttribute('aria-expanded') !== 'true';
    toggle.setAttribute('aria-expanded', String(expanded));
    details.hidden = !expanded;
    panel.classList.toggle('is-expanded', expanded);
    return;
  }
  if (issue) { event.preventDefault(); focusQualityIssue(issue.dataset.dramaQualityFocus); }
});
