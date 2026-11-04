/**
 * Pill geometry. Phase 0 placeholder sizes; the authoritative numbers live in
 * docs/design-spec.md (Phase 1).
 */

export type PillState = 'idle' | 'expanded';

export interface Size {
  w: number;
  h: number;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Logical size of the overlay window. It must contain the largest state. */
export const WINDOW: Size = { w: 400, h: 220 };

/** Gap between the top screen edge and the pill on screens without a notch. */
export const TOP_GAP = 8;

export const SIZES: Record<PillState, Size> = {
  idle: { w: 126, h: 37 },
  expanded: { w: 372, h: 160 },
};

/** Rect of the pill in window coordinates, horizontally centred. */
export function pillRect(state: PillState, win: Size = WINDOW, topGap = TOP_GAP): Rect {
  const s = SIZES[state];
  if (s.w > win.w || s.h + topGap > win.h) {
    throw new RangeError(`pill state "${state}" does not fit in ${win.w}x${win.h}`);
  }
  return { x: Math.round((win.w - s.w) / 2), y: topGap, w: s.w, h: s.h };
}

/**
 * Scale factors that turn the expanded-size element into `state`.
 * The pill element is laid out once at its largest size and only ever
 * transformed, so no animation triggers layout.
 */
export function scaleFor(state: PillState): { sx: number; sy: number } {
  const from = SIZES.expanded;
  const to = SIZES[state];
  return { sx: to.w / from.w, sy: to.h / from.h };
}

/** Union of two rects; used so the hit region covers both ends of a transition. */
export function union(a: Rect, b: Rect): Rect {
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return {
    x,
    y,
    w: Math.max(a.x + a.w, b.x + b.w) - x,
    h: Math.max(a.y + a.h, b.y + b.h) - y,
  };
}
