/** Drag-to-resize behavior for the game graph canvas and node inspector. */

const RAIL_WIDTH = 87;
const RESIZER_WIDTH = 3;
const MIN_CANVAS_WIDTH = 360;
const MIN_INSPECTOR_WIDTH = 330;

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), Math.max(min, max));
}

/** Calculate a safe canvas width from a pointer position within the editor layout. */
export function gameCanvasWidthFromPointer(pointerX: number, layoutLeft: number, layoutWidth: number) {
  const maximum = layoutWidth - RAIL_WIDTH - RESIZER_WIDTH - MIN_INSPECTOR_WIDTH;
  return clamp(pointerX - layoutLeft - RAIL_WIDTH, MIN_CANVAS_WIDTH, maximum);
}

/** Bind the 3px canvas-edge drag target while preserving responsive single-column layouts. */
export function bindGameCanvasResize(root: ParentNode = document) {
  const layout = root.querySelector<HTMLElement>('.game-editor-layout');
  const handle = root.querySelector<HTMLElement>('[data-game-canvas-resizer]');
  const canvas = root.querySelector<HTMLElement>('.game-canvas-panel');
  if (!layout || !handle || !canvas || window.matchMedia('(max-width: 1250px)').matches) return;
  const apply = (width: number) => {
    const bounds = layout.getBoundingClientRect();
    const safeWidth = gameCanvasWidthFromPointer(bounds.left + RAIL_WIDTH + width, bounds.left, bounds.width);
    layout.style.setProperty('--game-canvas-width', `${safeWidth}px`);
    layout.dataset.canvasResized = 'true';
    return safeWidth;
  };
  apply(canvas.getBoundingClientRect().width / 2);
  handle.addEventListener('pointerdown', event => {
    event.preventDefault();
    handle.setPointerCapture(event.pointerId);
    layout.classList.add('is-canvas-resizing');
    const resize = (pointerX: number) => apply(gameCanvasWidthFromPointer(pointerX, layout.getBoundingClientRect().left, layout.getBoundingClientRect().width));
    const finish = () => {
      layout.classList.remove('is-canvas-resizing');
      handle.removeEventListener('pointermove', move);
      handle.removeEventListener('pointerup', finish);
      handle.removeEventListener('pointercancel', finish);
    };
    const move = (moveEvent: PointerEvent) => resize(moveEvent.clientX);
    handle.addEventListener('pointermove', move);
    handle.addEventListener('pointerup', finish, { once: true });
    handle.addEventListener('pointercancel', finish, { once: true });
  });
}
