/** Shared public image-prompt defaults for interactive-game material types. */

import type { Game } from './models.js';

export type GameAssetPublicPromptKind = 'character' | 'scene' | 'prop';

/** Return the visible fallback shared-image instruction for one material type. */
export function gameAssetPublicPromptDefault(game: Game, kind: GameAssetPublicPromptKind) {
  const style = game.style || '真人风格';
  if (kind === 'character') return `图片风格为「${style}」，生成完整角色设定板（character turnaround and expression sheet），规整多格排版；不要左右二分构图，不要只生成头像和单张全身像。第一排放同一角色三视图：正面、严格侧面、背面，均为从头到鞋子的全身站立视图；第二排六个等尺寸的表情特写：自然、微笑、悲伤、惊讶、生气、委屈；第三排四个全身动作：行走、奔跑或抬手、开心互动、害羞遮脸。所有格子严格服从当前素材提示词指定的角色形态；同一张图内保持同一张脸、该形态对应的年龄、发型、妆容、体型、服装和配饰，禁止把幼年、成年或其他形态混在一张图中；灰色摄影棚背景，柔和均匀布光，边界清晰，人物不重叠、不裁切、不变形，无文字、水印或多余人物。`;
  if (kind === 'scene') return `图片风格为「${style}」，场景设定图需明确空间结构、时间氛围、关键光源和可供角色活动的区域；不要出现文字、水印或 UI。`;
  return `图片风格为「${style}」，道具设定图需完整展示轮廓、材质、尺寸关系和关键细节；背景干净，便于后续镜头反复引用。`;
}

/** Prefer the creator's saved instruction and otherwise expose the system default in the editor. */
export function gameAssetPublicPrompt(game: Game, kind: GameAssetPublicPromptKind) {
  return game.asset_public_prompts?.[kind]?.trim() || gameAssetPublicPromptDefault(game, kind);
}
