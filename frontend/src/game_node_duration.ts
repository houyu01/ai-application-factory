const GAME_NODE_DURATION_VALUES = [5, 6, 7, 8, 9, 10];

/** Render the supported five-to-ten-second duration choices for a game-node video. */
export function gameNodeDurationOptions(value: unknown) {
  const selected = Math.min(10, Math.max(5, Math.round(Number(value) || 10)));
  return GAME_NODE_DURATION_VALUES.map(seconds => `<option value="${seconds}"${seconds === selected ? ' selected' : ''}>${seconds} 秒</option>`).join('');
}
