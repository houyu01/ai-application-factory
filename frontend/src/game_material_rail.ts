/** Pure markup for the interactive-game material rail, shared by the workbench and UI checks. */

export type GameMaterialAssetKind = 'character' | 'scene' | 'prop';
export type GameMaterialRailKind = GameMaterialAssetKind | 'frames' | 'placeholder' | 'cover';

export const gameAssetKinds: { type: GameMaterialAssetKind; label: string; icon: string }[] = [
  { type: 'character', label: '角色', icon: '<i class="game-material-role-icon"></i>' },
  { type: 'scene', label: '场景', icon: '<i class="game-material-scene-icon"></i>' },
  { type: 'prop', label: '道具', icon: '<i class="game-material-prop-icon"></i>' },
];
export const gameMaterialRailItems: { type: GameMaterialRailKind; label: string; icon: string }[] = [
  ...gameAssetKinds,
  { type: 'frames', label: '首尾帧', icon: '<i class="game-material-frame-icon"></i>' },
  { type: 'placeholder', label: '占位图', icon: '<i class="game-material-placeholder-icon"></i>' },
  { type: 'cover', label: '封面', icon: '<i class="game-material-cover-icon"></i>' },
];

export function gameMaterialLabel(type: string) { return gameMaterialRailItems.find(item => item.type === type)?.label || type; }

export function gameMaterialRailMarkup() {
  return `<aside class="drama-asset-rail game-material-rail" aria-label="互动游戏素材配置">${gameMaterialRailItems.map(item => `<button type="button" class="drama-asset-rail-item" data-game-open-material="${item.type}" title="打开${item.label}配置"><span class="drama-asset-rail-icon">${item.icon}</span><span>${item.label}</span></button>`).join('')}</aside>`;
}
