// The page's one moment: a soft light on the hero's lit ground that follows
// the pointer while it is over the picture, and settles back when it leaves.
// Nothing moves for touch, keyboard or reduced motion.

/** Where the light sits, in percent of the box, for a pointer at (x, y). */
export function spot(box, x, y) {
  const clamp = (v) => Math.min(100, Math.max(0, v));
  return { x: clamp(((x - box.left) / box.width) * 100), y: clamp(((y - box.top) / box.height) * 100) };
}

if (typeof document !== 'undefined' && matchMedia('(hover: hover) and (prefers-reduced-motion: no-preference)').matches) {
  for (const el of document.querySelectorAll('[data-spotlight]')) {
    let frame = 0;
    el.addEventListener('pointermove', (e) => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const p = spot(el.getBoundingClientRect(), e.clientX, e.clientY);
        el.style.setProperty('--sx', `${p.x}%`);
        el.style.setProperty('--sy', `${p.y}%`);
      });
    });
    el.addEventListener('pointerleave', () => {
      cancelAnimationFrame(frame);
      el.style.removeProperty('--sx');
      el.style.removeProperty('--sy');
    });
  }
}
