/** Renders and refreshes selected shot references without owning their source assets. */
import type { ApiProject, DramaAsset, DramaPromptNode, DramaShot, GenerationTask } from './models.js';
import { dramaAssets, dramaKindLabel, dramaShotReferences, resolveMediaUrl } from './drama_core_ui.js';
import { renderDramaReferenceCards } from './drama_reference_cards.js';
import { dramaReferenceAsset, dramaReferenceKey } from './drama_reference_asset.js';
import { dramaViewState } from './drama_state.js';
import { icon } from './ui_icons.js';

type ReferenceNode = Extract<DramaPromptNode, { type: 'reference' }>;

type PanelOptions = {
  project: ApiProject;
  shot: DramaShot;
  referenceImageTask?: GenerationTask;
  escapeHtml: (value: unknown) => string;
  setTaskButtonLoading: (button: HTMLButtonElement, task: GenerationTask | undefined, idleText: string) => void;
};

function renderCards(shotId: string, references: ReferenceNode[], assets: Map<string, DramaAsset>, tasks: GenerationTask[], escapeHtml: PanelOptions['escapeHtml'], showStatus: boolean) {
  return renderDramaReferenceCards({
    shotId, references, assets, tasks, escapeHtml, resolveMediaUrl,
    kindLabel: dramaKindLabel, trashIcon: icon('trash'), showStatus,
  });
}

function uniqueMaterialReferences(references: ReferenceNode[]) {
  const seen = new Set<string>();
  return references.filter(reference => {
    const key = dramaReferenceKey(reference) || reference.image_url || reference.label;
    if (!key || seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

/** Keep the warning and its actions synchronized with the selected shot's persisted references. */
export function renderDramaShotReferencePanel(options: PanelOptions) {
  const { project, shot, referenceImageTask, escapeHtml, setTaskButtonLoading } = options;
  const references = uniqueMaterialReferences(dramaShotReferences(project, shot));
  const assets = new Map(dramaAssets(project).map(asset => [asset.id, asset]));
  const referencePanel = document.querySelector<HTMLElement>('.drama-shot-editor .drama-reference-panel');
  const grid = referencePanel?.querySelector<HTMLElement>('.drama-reference-grid');
  if (grid) grid.innerHTML = renderCards(shot.id, references, assets, project.tasks || [], escapeHtml, true);
  const existingStatus = referencePanel?.querySelector<HTMLElement>('[data-drama-reference-status]');
  if (!references.length) {
    existingStatus?.remove();
    return;
  }
  if (!referencePanel) return;
  const missing = references.filter(reference => {
    const asset = dramaReferenceAsset([...assets.values()], reference);
    return !asset || asset.status !== '生成成功' || !asset.image_url;
  });
  const status = existingStatus || document.createElement('div');
  status.className = `drama-reference-status ${missing.length ? 'has-warning' : 'is-ready'}`;
  status.dataset.dramaReferenceStatus = 'true';
  const actions = missing.length
    ? '<div class="drama-reference-status-actions"><button type="button" class="ghost compact" data-drama-generate-reference-images>一键生成参考图</button><button type="button" class="ghost compact" data-drama-auto-match>自动匹配参考图</button></div>'
    : '<div class="drama-reference-status-actions"><button type="button" class="ghost compact" data-drama-auto-match>自动匹配参考图</button></div>';
  status.innerHTML = `${missing.length ? `<span>⚠ 有 ${missing.length} 个参考素材不可用，生成视频前请先生成图片或重新选择参考图。</span>` : `<span>✓ 当前参考素材已就绪（${references.length} 项）</span>`}${actions}`;
  if (!status.parentElement) referencePanel.append(status);
  const generateButton = status.querySelector<HTMLButtonElement>('[data-drama-generate-reference-images]');
  if (generateButton) setTaskButtonLoading(generateButton, referenceImageTask, '一键生成参考图');
}

/** Refresh only reference cards while another editor interaction updates their rich-prompt nodes. */
export function syncDramaShotReferenceCards(project: ApiProject, escapeHtml: (value: unknown) => string) {
  const grid = document.querySelector<HTMLElement>('.drama-reference-grid');
  const selectedShot = project.shots?.find(item => item.id === dramaViewState.shotId) || project.shots?.[0];
  if (!grid || !selectedShot) return;
  const references = uniqueMaterialReferences(dramaShotReferences(project, selectedShot));
  const signature = references.map(node => { const asset = dramaReferenceAsset(project.assets || [], node); return `${dramaReferenceKey(node)}:${node.asset_type}:${node.image_url || ''}:${asset?.status || ''}`; }).join('|');
  if (grid.dataset.referenceSignature === signature) return;
  grid.dataset.referenceSignature = signature;
  const assets = new Map(dramaAssets(project).map(asset => [asset.id, asset]));
  grid.innerHTML = renderCards(selectedShot.id, references, assets, project.tasks || [], escapeHtml, false);
}
