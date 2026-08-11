/** Normalize durable game-node video tasks and completed history into one version list. */

import type { GameNode, GameNodeVideoHistory, GameTask } from './models.js';

export type GameNodeVideoRecord = {
  id: string;
  createdAt?: string;
  error?: string | null;
  progress?: number;
  refinementPrompt?: string | null;
  status: string;
  taskId?: string;
  url?: string | null;
};

const versionUrl = new Map<string, string>();
const nodeKey = (node: GameNode) => node.id;

/** Format an ISO-like history timestamp as the second-level value shown on a video-version card. */
export function gameNodeVideoHistoryTime(value?: string) {
  const match = value?.match(/^(\d{4}-\d{2}-\d{2})[T\s](\d{2}:\d{2}:\d{2})/);
  return match ? `${match[1]} ${match[2]}` : '';
}

function record(video: GameNodeVideoHistory): GameNodeVideoRecord {
  return {
    id: String(video.id || video.task_id || video.url || ''),
    createdAt: video.generated_at,
    error: video.error_message,
    refinementPrompt: video.refinement_prompt,
    status: video.status || (video.url ? '生成成功' : '未生成'),
    taskId: video.task_id,
    url: video.url,
  };
}

/** Return newest first, adding the current durable task before its terminal record is persisted. */
export function gameNodeVideoHistoryRecords(node: GameNode, task?: GameTask): GameNodeVideoRecord[] {
  const records = (node.video_history || []).map(record);
  if (task?.status === '生成中' && !records.some(item => item.id === task.id || item.taskId === task.id)) {
    records.push({
      id: task.id,
      createdAt: task.created_at,
      error: task.error_message,
      progress: task.progress,
      refinementPrompt: typeof task.input_snapshot?.refinement === 'object'
        ? String((task.input_snapshot.refinement as Record<string, unknown>).prompt || '')
        : undefined,
      status: task.status,
      taskId: task.id,
    });
  }
  return records
    .map((item, index) => ({ item, index }))
    .sort((left, right) => {
      const difference = Date.parse(right.item.createdAt || '') - Date.parse(left.item.createdAt || '');
      return Number.isFinite(difference) && difference ? difference : right.index - left.index;
    })
    .map(({ item }) => item);
}

/** Return the creator's durable current-version choice when its completed history record is still available. */
export function selectedGameNodeVideoId(node: GameNode): string | null {
  const selected = node.selected_video_id || '';
  return selected && (node.video_history || []).some(video => video.id === selected && video.status === '生成成功' && Boolean(video.url))
    ? selected
    : null;
}

/** Resolve the preview from a temporary history-card choice, then from the creator's durable runtime choice. */
export function selectedGameNodeVideoUrl(node: GameNode): string {
  const selected = versionUrl.get(nodeKey(node));
  if (selected && (node.video_history || []).some(video => video.url === selected)) return selected;
  const selectedId = selectedGameNodeVideoId(node);
  const selectedVersion = selectedId ? (node.video_history || []).find(video => video.id === selectedId) : undefined;
  if (selectedVersion?.url) return selectedVersion.url;
  return node.video_url || [...(node.video_history || [])].reverse().find(video => video.url)?.url || '';
}

export function selectGameNodeVideoUrl(node: GameNode, url?: string | null) {
  if (url) versionUrl.set(nodeKey(node), url);
  else versionUrl.delete(nodeKey(node));
}
