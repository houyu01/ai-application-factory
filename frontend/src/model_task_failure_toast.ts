/** Shows each durable model-task failure once, without flooding the creator during batch runs. */

type FailedModelTask = {
  id: string;
  type: string;
  status: string;
  error_message?: string | null;
  stage?: string | null;
};

const reported = new Map<string, string>();
const labels: Record<string, string> = {
  script_decomposition: '剧本生成', script_expansion: '剧本扩写', shot_prompt: '分镜提示词生成',
  shot_quality: '分镜质量检查', asset_image: '素材图片生成', asset_variant_image: '素材形态图片生成',
  asset_image_batch: '素材图片批量生成', shot_reference_image_batch: '分镜参考图批量生成',
  placeholder_image: '占位图生成', cover_image: '封面生成', shot_video: '分镜视频生成',
  game_graph_decomposition: '互动游戏图谱生成', node_video_generation: '互动游戏节点视频生成',
};

function failureText(task: FailedModelTask) {
  return task.error_message?.trim() || task.stage?.trim() || '模型未返回可用结果，请检查对应模型配置后重试';
}

/** Mark failures already visible in a loaded project so reopening it does not replay stale error toasts. */
export function suppressExistingModelTaskFailureNotifications(tasks: FailedModelTask[]) {
  tasks.forEach(task => {
    if (task.status !== '生成失败') return;
    const detail = failureText(task);
    reported.set(task.id, `${task.id}:${detail}`);
  });
}

/** Report terminal model failures from task polling or a freshly loaded project exactly once per failure detail. */
export function notifyModelTaskFailures(tasks: FailedModelTask[], toast: (message: string) => void) {
  const messages = tasks.flatMap(task => {
    if (task.status !== '生成失败') return [];
    const detail = failureText(task);
    const signature = `${task.id}:${detail}`;
    if (reported.get(task.id) === signature) return [];
    reported.set(task.id, signature);
    return [`${labels[task.type] || '模型任务'}失败：${detail}`];
  });
  messages.slice(0, 3).forEach(toast);
  if (messages.length > 3) toast(`另有 ${messages.length - 3} 个模型任务失败，可在对应项目中查看并重新生成。`);
}
