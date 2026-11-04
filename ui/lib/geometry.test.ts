import { describe, expect, it } from 'vitest';
import { pillRect, scaleFor, union, SIZES, WINDOW } from './geometry';

describe('pillRect', () => {
  it('centres the idle pill horizontally', () => {
    const r = pillRect('idle');
    expect(r.x * 2 + r.w).toBeCloseTo(WINDOW.w, 0);
    expect(r.w).toBe(SIZES.idle.w);
  });

  it('fits every state inside the window', () => {
    for (const s of ['idle', 'expanded'] as const) {
      const r = pillRect(s);
      expect(r.x).toBeGreaterThanOrEqual(0);
      expect(r.x + r.w).toBeLessThanOrEqual(WINDOW.w);
      expect(r.y + r.h).toBeLessThanOrEqual(WINDOW.h);
    }
  });

  it('rejects windows that are too small', () => {
    expect(() => pillRect('expanded', { w: 100, h: 100 })).toThrow(RangeError);
  });
});

describe('scaleFor', () => {
  it('is identity for the expanded state', () => {
    expect(scaleFor('expanded')).toEqual({ sx: 1, sy: 1 });
  });

  it('shrinks to idle', () => {
    const { sx, sy } = scaleFor('idle');
    expect(sx).toBeLessThan(1);
    expect(sy).toBeLessThan(1);
  });
});

describe('union', () => {
  it('covers both rects', () => {
    expect(union({ x: 10, y: 10, w: 10, h: 10 }, { x: 0, y: 15, w: 5, h: 20 })).toEqual({
      x: 0,
      y: 10,
      w: 20,
      h: 25,
    });
  });
});
