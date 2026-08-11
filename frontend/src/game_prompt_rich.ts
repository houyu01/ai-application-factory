/** Rich-text prompt helpers for interactive-game video nodes. */

import type { DramaPromptAssetType, DramaPromptNode, Game, GameNode } from './models.js';

type Runtime = { escapeHtml: (value: unknown) => string; resolveMediaUrl: (value?: string | null) => string };
type SerializedPrompt = { nodes: DramaPromptNode[]; prompt: string };

function referenceAssets(game: Game) {
  return (game.assets || []).filter(asset => ['character', 'scene', 'prop', 'placeholder'].includes(asset.type));
}

/** Lists the reusable game materials that can become short-drama-style prompt chips. */
export function gamePromptReferenceOptions(game: Game): Extract<DramaPromptNode, { type: 'reference' }>[] {
  return referenceAssets(game).map(asset => ({
    type: 'reference', asset_id: asset.id, asset_type: asset.type as DramaPromptAssetType,
    label: asset.name, image_url: asset.image_url || null,
  }));
}

function legacyPromptNodes(game: Game | undefined, prompt: string): DramaPromptNode[] {
  if (!game || !prompt) return [{ type: 'text', text: prompt }];
  const nodes: DramaPromptNode[] = [];
  const pattern = /@图\d+[（(]([^）)]+)[）)]/g;
  let cursor = 0;
  for (let match = pattern.exec(prompt); match; match = pattern.exec(prompt)) {
    if (match.index > cursor) nodes.push({ type: 'text', text: prompt.slice(cursor, match.index) });
    const asset = referenceAssets(game).find(item => item.name === match[1]);
    if (asset) nodes.push({ type: 'reference', asset_id: asset.id, asset_type: asset.type as DramaPromptAssetType, label: asset.name, image_url: asset.image_url || null });
    else nodes.push({ type: 'text', text: match[0] });
    cursor = match.index + match[0].length;
  }
  if (cursor < prompt.length) nodes.push({ type: 'text', text: prompt.slice(cursor) });
  return nodes.length ? nodes : [{ type: 'text', text: prompt }];
}

/** Restores saved game rich nodes, falling back to the legacy plain-text prompt. */
export function gamePromptNodes(node: GameNode, game?: Game): DramaPromptNode[] {
  return Array.isArray(node.prompt_rich) && node.prompt_rich.length
    ? node.prompt_rich
    : legacyPromptNodes(game, node.prompt || '');
}

/** Serializes game prompt chips to the provider-facing @图 text while refreshing their asset metadata. */
export function serializeGamePromptNodes(game: Game, nodes: DramaPromptNode[]): SerializedPrompt {
  let mentionNumber = 0;
  const mentionNumbers = new Map<string, number>();
  const normalized: DramaPromptNode[] = [];
  for (const node of nodes) {
    if (node.type === 'text') {
      if (node.text) normalized.push({ type: 'text', text: node.text });
      continue;
    }
    const asset = referenceAssets(game).find(item => item.id === node.asset_id);
    const assetType = (asset?.type || node.asset_type || 'placeholder') as DramaPromptAssetType;
    if (!mentionNumbers.has(node.asset_id)) mentionNumbers.set(node.asset_id, ++mentionNumber);
    normalized.push({
      type: 'reference', asset_id: node.asset_id, asset_type: assetType,
      label: asset?.name || node.label || '占位图', image_url: asset?.image_url || node.image_url || null,
      mention_number: mentionNumbers.get(node.asset_id),
    });
  }
  return { nodes: normalized, prompt: normalized.map(node => node.type === 'text' ? node.text : `@图${node.mention_number}（${node.label}）`).join('').trim() };
}

/** Returns the unique material IDs referenced in one rich prompt. */
export function gamePromptReferenceAssetIds(nodes: DramaPromptNode[]) {
  return [...new Set(nodes.flatMap(node => node.type === 'reference' ? [node.asset_id] : []))];
}

/** Renders safe, non-editable @ reference chips into a game node contenteditable area. */
export function renderGamePromptNodes(root: HTMLElement, game: Game, runtime: Runtime, nodes: DramaPromptNode[]) {
  root.replaceChildren();
  for (const node of nodes) {
    if (node.type === 'text') { root.append(document.createTextNode(node.text)); continue; }
    const asset = referenceAssets(game).find(item => item.id === node.asset_id);
    const chip = document.createElement('span');
    chip.className = 'game-prompt-reference drama-prompt-reference';
    chip.contentEditable = 'false';
    chip.dataset.gamePromptReference = 'true';
    chip.dataset.assetId = node.asset_id;
    chip.dataset.assetType = asset?.type || node.asset_type;
    chip.dataset.label = asset?.name || node.label;
    chip.dataset.imageUrl = asset?.image_url || node.image_url || '';
    chip.dataset.mentionNumber = String(node.mention_number || 1);
    const imageUrl = runtime.resolveMediaUrl(asset?.image_url || node.image_url);
    if (imageUrl) {
      const image = document.createElement('img');
      image.src = imageUrl; image.alt = asset?.name || node.label; chip.append(image);
    } else {
      const placeholder = document.createElement('span');
      placeholder.className = 'drama-prompt-reference-placeholder';
      placeholder.textContent = node.asset_type === 'character' ? '♙' : node.asset_type === 'scene' ? '✦' : node.asset_type === 'prop' ? '◆' : '＋';
      chip.append(placeholder);
    }
    const label = document.createElement('span');
    label.textContent = `@图${node.mention_number || 1}（${asset?.name || node.label}）`;
    chip.append(label); root.append(chip);
  }
}

/** Reads text and protected game @ chips from a rich editor without HTML serialization. */
export function readGamePromptNodes(root: HTMLElement): DramaPromptNode[] {
  const nodes: DramaPromptNode[] = [];
  const visit = (node: Node) => {
    if (node.nodeType === Node.TEXT_NODE) {
      const text = node.textContent || '';
      if (text) nodes.push({ type: 'text', text });
      return;
    }
    if (!(node instanceof HTMLElement)) return;
    if (node.dataset.gamePromptReference === 'true') {
      nodes.push({ type: 'reference', asset_id: node.dataset.assetId || '', asset_type: (node.dataset.assetType || 'placeholder') as DramaPromptAssetType, label: node.dataset.label || '占位图', image_url: node.dataset.imageUrl || null, mention_number: Number(node.dataset.mentionNumber || 1) });
      return;
    }
    if (node.tagName === 'BR') { nodes.push({ type: 'text', text: '\n' }); return; }
    node.childNodes.forEach(visit);
  };
  root.childNodes.forEach(visit);
  return nodes;
}

type EditorOptions = {
  inspector: HTMLElement; game: Game; node: GameNode; runtime: Runtime;
  onUpdate: (prompt: SerializedPrompt) => void;
  openMentionPicker: (onComplete: (node: Extract<DramaPromptNode, { type: 'reference' }>) => void) => void;
};

/** Binds native text editing, @ insertion, and hidden source synchronization for one game node. */
export function bindGameRichPromptEditor(options: EditorOptions) {
  const source = options.inspector.querySelector<HTMLTextAreaElement>('#node-prompt');
  const editor = options.inspector.querySelector<HTMLElement>('.game-rich-prompt-editor');
  const frame = options.inspector.querySelector<HTMLElement>('.game-rich-prompt-frame');
  const toolbar = options.inspector.querySelector<HTMLElement>('[data-game-prompt-toolbar]');
  if (!source || !editor || !frame) return;
  let savedRange: Range | null = null;
  const sync = (notify = true) => {
    const serialized = serializeGamePromptNodes(options.game, readGamePromptNodes(editor));
    source.value = serialized.prompt;
    source.dataset.promptRich = JSON.stringify(serialized.nodes);
    frame.classList.toggle('has-content', Boolean(serialized.prompt));
    if (notify) options.onUpdate(serialized);
  };
  const rememberSelection = () => {
    const selection = window.getSelection();
    if (!selection?.rangeCount) return;
    const range = selection.getRangeAt(0);
    if (editor.contains(range.startContainer) && editor.contains(range.endContainer)) savedRange = range.cloneRange();
  };
  const insert = (reference: Extract<DramaPromptNode, { type: 'reference' }>) => {
    editor.focus();
    const range = savedRange && editor.contains(savedRange.startContainer) && editor.contains(savedRange.endContainer)
      ? savedRange.cloneRange()
      : (() => { const fallback = document.createRange(); fallback.selectNodeContents(editor); fallback.collapse(false); return fallback; })();
    range.deleteContents();
    const temporary = document.createElement('span');
    renderGamePromptNodes(temporary, options.game, options.runtime, [reference]);
    const chip = temporary.firstElementChild;
    if (!chip) return;
    range.insertNode(chip);
    const spacer = document.createTextNode(' ');
    range.setStartAfter(chip); range.collapse(true); range.insertNode(spacer);
    range.setStartAfter(spacer); range.collapse(true);
    const selection = window.getSelection(); selection?.removeAllRanges(); selection?.addRange(range);
    sync(); rememberSelection();
  };
  const addToolbarButton = (reference: Extract<DramaPromptNode, { type: 'reference' }>, text: string) => {
    if (!toolbar) return;
    const button = document.createElement('button');
    button.type = 'button'; button.className = 'drama-rich-prompt-reference-button';
    button.textContent = text; button.title = `插入${text}`;
    button.addEventListener('mousedown', event => event.preventDefault());
    button.addEventListener('click', () => insert(reference));
    toolbar.append(button);
  };
  if (toolbar) {
    const label = document.createElement('span');
    label.className = 'drama-rich-prompt-label'; label.textContent = '插入参考图：';
    toolbar.replaceChildren(label);
    gamePromptReferenceOptions(options.game).forEach(reference => {
      const kind = reference.asset_type === 'character' ? '角色' : reference.asset_type === 'scene' ? '场景' : reference.asset_type === 'prop' ? '道具' : '占位图';
      addToolbarButton(reference, `${kind} · ${reference.label}`);
    });
    const picker = document.createElement('button');
    picker.type = 'button'; picker.className = 'drama-rich-prompt-reference-button';
    picker.textContent = '＋ 选择参考图'; picker.title = '从素材中选择并插入参考图';
    picker.addEventListener('mousedown', event => event.preventDefault());
    picker.addEventListener('click', () => { rememberSelection(); options.openMentionPicker(insert); });
    toolbar.append(picker);
  }
  renderGamePromptNodes(editor, options.game, options.runtime, gamePromptNodes(options.node, options.game));
  sync(false);
  editor.addEventListener('input', () => sync());
  editor.addEventListener('mouseup', rememberSelection);
  editor.addEventListener('keyup', rememberSelection);
  editor.addEventListener('focus', rememberSelection);
  editor.addEventListener('beforeinput', event => {
    const input = event as InputEvent;
    if (input.data !== '@') return;
    event.preventDefault(); rememberSelection(); options.openMentionPicker(insert);
  });
}
