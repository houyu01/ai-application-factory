/** Game-node boundary-frame editor aligned with the short-drama frame-library flow. */

import { captureDramaVideoFrame } from './drama_video_frame_capture.js';
import { gameRelatedNodes, gameRelatedVideoFrameChoices, type RelatedVideoFrame } from './game_upstream_frame_choices.js';
import type { Game, GameFrameReference, GameNode } from './models.js';

type FrameSide = 'first' | 'last';
type Runtime = {
  apiBaseUrl: string;
  escapeHtml: (value: unknown) => string;
  resolveMediaUrl: (value?: string | null) => string;
  toast: (message: string) => void;
};
type FrameChoice = RelatedVideoFrame & { kind: 'library' | 'video' };

const assetFor = (game: Game, id?: string | null) => game.assets?.find(asset => asset.id === id);
const nodeFor = (game: Game, id?: string | null) => game.nodes?.find(node => node.id === id);
const escape = (rt: Runtime, value: unknown) => rt.escapeHtml(value);

function assetOptions(game: Game, selectedId?: string | null) {
  return `<option value="">不设置</option>${(game.assets || [])
    .filter(asset => !['cover', 'cover_reference'].includes(asset.type))
    .map(asset => `<option value="${asset.id}"${asset.id === selectedId ? ' selected' : ''}>${asset.name}</option>`)
    .join('')}`;
}

function previewMarkup(game: Game, frame: GameFrameReference | null | undefined, rt: Runtime) {
  if (frame?.url) return `<span class="drama-frame-ai-badge">AI生成</span><img src="${escape(rt, frame.url)}" alt="已选视频帧" />`;
  const asset = assetFor(game, frame?.asset_id);
  return asset?.image_url
    ? `<img src="${escape(rt, rt.resolveMediaUrl(asset.image_url))}" alt="${escape(rt, asset.name)}" />`
    : `<span>${asset ? escape(rt, asset.name) : '尚未设置'}</span>`;
}

function savedFrameChoices(game: Game, node: GameNode): FrameChoice[] {
  return gameRelatedNodes(game, node).flatMap(({ node: related, relation }) => (['first', 'last'] as const).flatMap(position => {
    const frame = related.first_last_frames?.[position];
    if (!frame?.url || !frame.node_id || !frame.video_id || !frame.position) return [];
    return [{
      kind: 'library' as const,
      nodeId: frame.node_id,
      nodeTitle: related.title,
      position: frame.position,
      relation,
      url: frame.url,
      videoId: frame.video_id,
      videoLabel: `已绑定${position === 'first' ? '首' : '尾'}帧`,
    }];
  }));
}

function isSelected(frame: GameFrameReference | null | undefined, choice: FrameChoice) {
  return frame?.node_id === choice.nodeId
    && frame?.video_id === choice.videoId
    && frame?.position === choice.position;
}

function choiceMarkup(side: FrameSide, choice: FrameChoice, rt: Runtime, selected: boolean) {
  const relation = choice.relation === 'upstream' ? '上游' : '下游';
  const thumb = choice.kind === 'video'
    ? `<span class="drama-frame-choice-thumb" data-game-frame-thumb-url="${escape(rt, choice.url)}" data-game-frame-thumb-side="${choice.position}"><span class="drama-frame-ai-badge">AI生成</span><span>提取中…</span></span>`
    : `<span class="drama-frame-choice-thumb"><span class="drama-frame-ai-badge">AI生成</span><img src="${escape(rt, choice.url)}" alt="" /></span>`;
  return `<button type="button" class="drama-frame-choice-card ${selected ? 'selected' : ''}" data-game-frame-choice="${side}" data-game-choice-kind="${choice.kind}" data-game-source-node="${escape(rt, choice.nodeId)}" data-game-source-video="${escape(rt, choice.videoId)}" data-game-source-position="${choice.position}" data-game-source-url="${escape(rt, choice.url)}">${thumb}<span class="drama-frame-choice-label">${relation} · ${escape(rt, choice.nodeTitle)} · ${escape(rt, choice.videoLabel)}<small>${choice.kind === 'video' ? `取视频${choice.position === 'first' ? '首' : '尾'}帧` : '已绑定帧'}</small></span></button>`;
}

function sourcePanel(game: Game, node: GameNode, side: FrameSide, frame: GameFrameReference | null | undefined, rt: Runtime) {
  const library = savedFrameChoices(game, node);
  const videos = gameRelatedVideoFrameChoices(game, node).map(item => ({ ...item, kind: 'video' as const }));
  const cards = (choices: FrameChoice[]) => choices
    .map(choice => choiceMarkup(side, choice, rt, isSelected(frame, choice)))
    .join('') || '<small>暂无可用帧</small>';
  const sideName = side === 'first' ? '首' : '尾';
  return `<div class="drama-frame-source-panel" data-game-frame-source-panel="${side}" hidden><div class="drama-frame-source-head"><div><h4>选择输入${sideName}帧</h4><p>可选择与当前节点存在上下游关系的节点视频首帧和尾帧</p></div><button type="button" class="drama-frame-source-close" data-game-frame-source-close="${side}" aria-label="关闭">×</button></div><div class="drama-frame-source-tabs"><button type="button" class="active" data-game-frame-tab="${side}-library">▧ 可用首尾帧 <b>${library.length}</b></button><button type="button" data-game-frame-tab="${side}-video">▣ 从视频提取 <b>${videos.length}</b></button></div><div class="drama-frame-choice-grid">${cards(library)}</div><div class="drama-frame-choice-grid" data-game-frame-video-grid="${side}" hidden>${cards(videos)}</div></div>`;
}

function frameCard(game: Game, node: GameNode, side: FrameSide, frame: GameFrameReference | null | undefined, rt: Runtime) {
  const sideName = side === 'first' ? '首' : '尾';
  return `<section class="drama-frame-editor-card"><h3>输入${sideName}帧</h3><div class="drama-frame-preview" data-game-frame-preview="${side}">${previewMarkup(game, frame, rt)}</div><div class="drama-frame-actions"><button type="button" class="ghost compact" data-game-frame-library="${side}">▧ 从${sideName}帧库选择</button><label class="ghost compact">↥ 上传图片<input type="file" accept="image/*" data-game-frame-upload="${side}" hidden /></label><label class="ghost compact">选择素材图<select data-game-frame-asset="${side}">${assetOptions(game, frame?.asset_id)}</select></label></div>${sourcePanel(game, node, side, frame, rt)}</section>`;
}

/** Open the game frame editor with generated-video frames from all related graph nodes. */
export function openGameFramesModal(game: Game, rt: Runtime, refresh: () => Promise<void>, requestedNodeId?: string) {
  const initialNode = nodeFor(game, requestedNodeId) || game.nodes?.[0];
  if (!initialNode) { rt.toast('请等待视频节点生成后再配置首尾帧'); return; }
  const modal = document.createElement('div');
  modal.className = 'modal-backdrop game-frame-backdrop';
  modal.innerHTML = `<div class="modal drama-frame-modal"><button type="button" class="close" aria-label="关闭">×</button><div class="modal-head"><h2 data-game-frame-title></h2><p>首尾帧会作为普通参考图与当前节点素材一起发送，并由提示词约束视频的起止画面。</p></div><label class="game-material-node-picker">视频节点<select data-game-frame-node>${(game.nodes || []).map(node => `<option value="${escape(rt, node.id)}"${node.id === initialNode.id ? ' selected' : ''}>${escape(rt, node.title)}</option>`).join('')}</select></label><div class="drama-frame-editor-grid" data-game-frame-editor></div><div class="modal-actions"><button type="button" class="ghost" data-game-frame-clear>清除首尾帧</button><button type="button" class="primary" data-game-frame-save>完成</button></div></div>`;
  document.body.append(modal);
  const close = () => modal.remove();
  const picker = modal.querySelector<HTMLSelectElement>('[data-game-frame-node]')!;
  let node = initialNode;
  let frames: Record<FrameSide, GameFrameReference | null> = { first: node.first_last_frames?.first || null, last: node.first_last_frames?.last || null };
  const render = () => {
    modal.querySelector<HTMLElement>('[data-game-frame-title]')!.textContent = `首尾帧 · ${node.title}`;
    modal.querySelector<HTMLElement>('[data-game-frame-editor]')!.innerHTML = (['first', 'last'] as const).map(side => frameCard(game, node, side, frames[side], rt)).join('');
    bindEditor();
  };
  const bindEditor = () => {
    modal.querySelectorAll<HTMLElement>('[data-game-frame-library]').forEach(button => button.addEventListener('click', () => {
      const panel = modal.querySelector<HTMLElement>(`[data-game-frame-source-panel="${button.dataset.gameFrameLibrary}"]`);
      if (panel) panel.hidden = !panel.hidden;
    }));
    modal.querySelectorAll<HTMLElement>('[data-game-frame-source-close]').forEach(button => button.addEventListener('click', () => {
      const panel = modal.querySelector<HTMLElement>(`[data-game-frame-source-panel="${button.dataset.gameFrameSourceClose}"]`);
      if (panel) panel.hidden = true;
    }));
    modal.querySelectorAll<HTMLElement>('[data-game-frame-tab]').forEach(button => button.addEventListener('click', () => {
      const [side, kind] = (button.dataset.gameFrameTab || '').split('-') as [FrameSide, 'library' | 'video'];
      modal.querySelectorAll(`[data-game-frame-tab^="${side}-"]`).forEach(item => item.classList.toggle('active', item === button));
      modal.querySelectorAll<HTMLElement>(`[data-game-frame-choice="${side}"]`).forEach(item => { item.hidden = item.dataset.gameChoiceKind !== kind; });
      modal.querySelector<HTMLElement>(`[data-game-frame-video-grid="${side}"]`)!.hidden = kind !== 'video';
    }));
    modal.querySelectorAll<HTMLElement>('[data-game-frame-thumb-url]').forEach(async thumb => {
      const image = await captureDramaVideoFrame(thumb.dataset.gameFrameThumbUrl || '', thumb.dataset.gameFrameThumbSide as FrameSide, rt.resolveMediaUrl);
      if (image) thumb.innerHTML = `<span class="drama-frame-ai-badge">AI生成</span><img src="${image}" alt="" />`;
      else thumb.textContent = '无法提取';
    });
    modal.querySelectorAll<HTMLSelectElement>('[data-game-frame-asset]').forEach(field => field.addEventListener('change', () => {
      const side = field.dataset.gameFrameAsset as FrameSide;
      frames[side] = field.value ? { asset_id: field.value } : null;
      render();
    }));
    modal.querySelectorAll<HTMLInputElement>('[data-game-frame-upload]').forEach(input => input.addEventListener('change', () => {
      const file = input.files?.[0];
      if (!file) return;
      const reader = new FileReader();
      reader.onload = () => { frames[input.dataset.gameFrameUpload as FrameSide] = { url: String(reader.result || ''), source: 'upload' }; render(); };
      reader.readAsDataURL(file);
    }));
    modal.querySelectorAll<HTMLElement>('[data-game-frame-choice]').forEach(button => button.addEventListener('click', async () => {
      const side = button.dataset.gameFrameChoice as FrameSide;
      const position = button.dataset.gameSourcePosition as FrameSide;
      const image = button.querySelector<HTMLImageElement>('img')?.src || await captureDramaVideoFrame(button.dataset.gameSourceUrl || '', position, rt.resolveMediaUrl);
      if (!image) { rt.toast('无法提取该视频帧，请检查视频后重试'); return; }
      frames[side] = { url: image, source: 'related_video', node_id: button.dataset.gameSourceNode, video_id: button.dataset.gameSourceVideo, position };
      render();
    }));
  };
  picker.addEventListener('change', () => { node = nodeFor(game, picker.value) || initialNode; frames = { first: node.first_last_frames?.first || null, last: node.first_last_frames?.last || null }; render(); });
  modal.querySelector('.close')?.addEventListener('click', close);
  modal.querySelector('[data-game-frame-clear]')?.addEventListener('click', () => { frames = { first: null, last: null }; render(); });
  modal.querySelector('[data-game-frame-save]')?.addEventListener('click', async event => {
    const button = event.currentTarget as HTMLButtonElement;
    button.disabled = true;
    button.textContent = '保存中…';
    try {
      const response = await fetch(`${rt.apiBaseUrl}/games/${game.id}/nodes/${node.id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ first_last_frames: frames }) });
      if (!response.ok) throw new Error(await response.json().then(value => value.detail || `HTTP ${response.status}`).catch(() => `HTTP ${response.status}`));
      node.first_last_frames = { ...frames };
      close();
      rt.toast('首尾帧已保存');
      await refresh();
    } catch (error) {
      button.disabled = false;
      button.textContent = '完成';
      rt.toast(`首尾帧保存失败：${error instanceof Error ? error.message : '请稍后重试'}`);
    }
  });
  render();
}
