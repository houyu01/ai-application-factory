/** Shot management actions used by the episode/shot navigation column. */
import { dramaViewState } from './drama_state.js';
import { confirmAction } from './confirmation_modal.js';

type DramaShotRuntime = {
  apiBaseUrl: string;
  toast: (message: string) => void;
  loadDramaDetail: (id: string, retry?: number) => Promise<void>;
};

let runtime: DramaShotRuntime;
const rt = () => runtime;

export function configureDramaShotRuntime(value: DramaShotRuntime) {
  runtime = value;
}

async function readError(response: Response) {
  try {
    const body = await response.json() as { detail?: string };
    return body.detail || `HTTP ${response.status}`;
  } catch {
    return `HTTP ${response.status}`;
  }
}

/** Inserts a blank shot immediately after the shot whose plus button was used. */
export async function addDramaShot(projectId: string, afterShotId: string) {
  if (!afterShotId) return;
  try {
    const response = await fetch(`${rt().apiBaseUrl}/projects/${projectId}/shots`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ after_shot_id: afterShotId }),
    });
    if (!response.ok) throw new Error(await readError(response));
    const shot = await response.json() as { id?: string };
    dramaViewState.shotId = shot.id || null;
    dramaViewState.videoUrl = null;
    rt().toast('已新增空分镜');
    await rt().loadDramaDetail(projectId);
  } catch (error) {
    rt().toast(`新增分镜失败：${error instanceof Error ? error.message : '请求失败'}`);
  }
}

/** Deletes a shot and asks the local Tauri service to cancel its active generation task. */
export async function deleteDramaShot(projectId: string, shotId: string) {
  if (!shotId || !await confirmAction({ title: '删除分镜？', description: '删除后，相关视频版本、占位图和生成任务都会被一并清理，且无法恢复。', confirmLabel: '删除分镜' })) return;
  try {
    const response = await fetch(`${rt().apiBaseUrl}/projects/${projectId}/shots/${shotId}`, { method: 'DELETE' });
    if (!response.ok) throw new Error(await readError(response));
    const result = await response.json() as { next_shot_id?: string };
    dramaViewState.shotId = result.next_shot_id || null;
    dramaViewState.videoUrl = null;
    rt().toast('分镜已删除');
    await rt().loadDramaDetail(projectId);
  } catch (error) {
    rt().toast(`删除分镜失败：${error instanceof Error ? error.message : '请求失败'}`);
  }
}
