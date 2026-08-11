/** Resolve one shot's durable and legacy video records for preview and history UI. */
import type { DramaShot, DramaShotVersion, GenerationTask } from './models.js';

export type DramaVideoHistoryRecord = {
  id: string;
  createdAt?: string;
  error?: string | null;
  progress?: number;
  providerTaskId?: string | null;
  refinementPrompt?: string | null;
  selectedForExport?: boolean;
  status: string;
  taskId?: string | null;
  url?: string | null;
  versionNo?: number;
};

type DramaVideoTaskState = Pick<GenerationTask, 'status'>
  & Partial<Pick<GenerationTask, 'id' | 'error_message'>>;

function historyKeys(...values: Array<string | null | undefined>): string[] {
  return [...new Set(values.filter((value): value is string => Boolean(value)).map(String))];
}

function versionRecord(version: DramaShotVersion, knownKeys: Set<string>): DramaVideoHistoryRecord {
  const id = String(version.id || version.task_id || version.video_url || '');
  historyKeys(version.id, version.task_id, version.video_url).forEach(key => knownKeys.add(key));
  return {
    id,
    createdAt: version.completed_at || version.created_at,
    error: version.error_message,
    progress: version.progress,
    providerTaskId: version.provider_task_id,
    refinementPrompt: version.refinement_prompt,
    selectedForExport: Boolean(version.is_selected_for_export),
    status: version.status,
    taskId: version.task_id,
    url: version.video_url,
    versionNo: version.version_no,
  };
}

/** Return history newest first, with durable version numbers taking precedence over timestamps. */
export function dramaVideoHistoryRecords(shot: DramaShot): DramaVideoHistoryRecord[] {
  const knownKeys = new Set<string>();
  const records = (shot.versions || []).map(version => versionRecord(version, knownKeys));
  for (const video of shot.historical_videos || []) {
    const id = String(video.id || video.task_id || video.url || '');
    const keys = historyKeys(video.id, video.task_id, video.url);
    if (!id || keys.some(key => knownKeys.has(key))) continue;
    keys.forEach(key => knownKeys.add(key));
    records.push({ id, createdAt: video.generated_at, status: video.url ? '生成成功' : '未生成', taskId: video.task_id, url: video.url });
  }
  return records
    .map((record, index) => ({ record, index }))
    .sort((left, right) => {
      const versionDifference = (right.record.versionNo ?? -1) - (left.record.versionNo ?? -1);
      if (versionDifference) return versionDifference;
      const timeDifference = Date.parse(right.record.createdAt || '') - Date.parse(left.record.createdAt || '');
      if (Number.isFinite(timeDifference) && timeDifference) return timeDifference;
      return right.index - left.index;
    })
    .map(({ record }) => record);
}

/** Return only the newest video generation, preferring a live task before its version refreshes. */
export function latestDramaVideoGeneration(shot: DramaShot, videoTask?: DramaVideoTaskState): DramaVideoHistoryRecord | null {
  if (videoTask?.status === '生成中') {
    return {
      id: videoTask.id || '',
      error: videoTask.error_message,
      status: videoTask.status,
      taskId: videoTask.id,
    };
  }
  const latestVersion = dramaVideoHistoryRecords(shot)[0];
  if (latestVersion) return latestVersion;
  return videoTask
    ? { id: videoTask.id || '', error: videoTask.error_message, status: videoTask.status, taskId: videoTask.id }
    : null;
}

/** Derive visible state from the newest generation only, so old failures cannot leak into the current preview. */
export function dramaShotVideoStatus(shot: DramaShot, videoTask?: DramaVideoTaskState): string {
  return latestDramaVideoGeneration(shot, videoTask)?.status || '未生成';
}

/** Pick the newest playable video when the user has not explicitly selected a history card. */
export function latestDramaVideoUrl(shot?: DramaShot): string | null {
  return shot
    ? dramaVideoHistoryRecords(shot).find(record => Boolean(record.url) && record.status !== '生成失败' && record.status !== '已取消')?.url || null
    : null;
}
