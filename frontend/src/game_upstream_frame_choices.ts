/** Resolve playable ancestor and descendant video frames for the game frame editor. */

import type { Game, GameNode } from './models.js';

export type RelatedVideoFrame = {
  nodeId: string;
  nodeTitle: string;
  position: 'first' | 'last';
  relation: 'upstream' | 'downstream';
  url: string;
  videoId: string;
  videoLabel: string;
};

export type RelatedGameNode = { node: GameNode; relation: 'upstream' | 'downstream' };

function nodeVideos(node: GameNode) {
  const history = (node.video_history || [])
    .filter(video => video.status === '生成成功' && Boolean(video.url))
    .map((video, index) => ({ id: String(video.id || video.task_id || index), url: video.url || '', label: `版本 ${index + 1}` }));
  const knownUrls = new Set(history.map(video => video.url));
  const selectedUrl = node.selected_video_id
    ? history.find(video => video.id === node.selected_video_id)?.url || node.video_url || ''
    : node.video_url || '';
  if (selectedUrl && !knownUrls.has(selectedUrl)) {
    history.push({ id: node.selected_video_id || 'current', url: selectedUrl, label: '当前使用版本' });
  }
  return history;
}

function reachableNodeIds(startId: string, game: Game, direction: 'upstream' | 'downstream') {
  const visited = new Set<string>();
  const pending = [startId];
  while (pending.length) {
    const current = pending.pop()!;
    const next = (game.edges || []).flatMap(edge => {
      if (direction === 'upstream' && edge.target_node_id === current) return [edge.source_node_id];
      if (direction === 'downstream' && edge.source_node_id === current) return [edge.target_node_id];
      return [];
    });
    next.forEach(id => { if (!visited.has(id)) { visited.add(id); pending.push(id); } });
  }
  return visited;
}

/** Return all graph ancestors and descendants, excluding unrelated branches and the current node. */
export function gameRelatedNodes(game: Game, node: GameNode): RelatedGameNode[] {
  const upstream = reachableNodeIds(node.id, game, 'upstream');
  const downstream = reachableNodeIds(node.id, game, 'downstream');
  return (game.nodes || []).reduce<RelatedGameNode[]>((related, item) => {
    if (upstream.has(item.id)) related.push({ node: item, relation: 'upstream' });
    else if (downstream.has(item.id)) related.push({ node: item, relation: 'downstream' });
    return related;
  }, []);
}

/** Offer both first and tail frames of every generated video on an upstream or downstream branch. */
export function gameRelatedVideoFrameChoices(game: Game, node: GameNode): RelatedVideoFrame[] {
  return gameRelatedNodes(game, node).flatMap(({ node: related, relation }) => nodeVideos(related)
    .flatMap(video => (['first', 'last'] as const).map(position => ({
      nodeId: related.id,
      nodeTitle: related.title,
      position,
      relation,
      url: video.url,
      videoId: video.id,
      videoLabel: video.label,
    }))));
}
