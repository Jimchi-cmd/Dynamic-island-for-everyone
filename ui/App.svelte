<script lang="ts">
  // Phase 0 pill: just enough to measure the stack's baseline cost.
  // The real morph (9-slice squircle + springs) replaces this in Phase 1.
  import { onMount } from 'svelte';
  import { pillRect, scaleFor, SIZES, type PillState } from './lib/geometry';
  import { bootConfig, setHitRegion, uiReady } from './lib/bridge';

  let state: PillState = $state('idle');
  const scale = $derived(scaleFor(state));
  const box = pillRect('expanded');

  function setState(next: PillState) {
    if (next === state) return;
    // Grow the input region before expanding so the growing pill is clickable;
    // shrink it only after the collapse animation ends (see onTransitionEnd).
    if (next === 'expanded') void setHitRegion(pillRect('expanded'));
    state = next;
  }

  function onTransitionEnd(e: TransitionEvent) {
    if (e.target === e.currentTarget && state === 'idle') void setHitRegion(pillRect('idle'));
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') setState('idle');
  }

  onMount(() => {
    let demo: ReturnType<typeof setInterval> | undefined;
    void setHitRegion(pillRect('idle'));
    // Two frames: the first rAF runs before paint, the second after it.
    requestAnimationFrame(() => requestAnimationFrame(() => void uiReady()));
    void bootConfig().then((cfg) => {
      if (cfg.perfDemo) {
        demo = setInterval(() => setState(state === 'idle' ? 'expanded' : 'idle'), 1200);
      }
    });
    return () => clearInterval(demo);
  });
</script>

<svelte:window onkeydown={onKey} />

<button
  class="pill"
  class:expanded={state === 'expanded'}
  style:left="{box.x}px"
  style:top="{box.y}px"
  style:width="{SIZES.expanded.w}px"
  style:height="{SIZES.expanded.h}px"
  style:transform="scale({scale.sx}, {scale.sy})"
  aria-label="Islet"
  aria-expanded={state === 'expanded'}
  onclick={() => setState(state === 'idle' ? 'expanded' : 'idle')}
  ontransitionend={onTransitionEnd}
>
  <span class="content" aria-hidden={state === 'idle'}>Islet</span>
</button>

<style>
  .pill {
    position: absolute;
    margin: 0;
    padding: 0;
    border: 0;
    background: #000;
    border-radius: 44px;
    transform-origin: 50% 0;
    /* Placeholder curve with slight overshoot; Phase 1 uses a real spring. */
    transition: transform 420ms cubic-bezier(0.32, 1.25, 0.5, 1);
    will-change: transform;
    color: #fff;
    font: 600 17px system-ui, sans-serif;
    outline: none;
  }
  .pill:focus-visible {
    box-shadow: 0 0 0 2px #fff;
  }
  .content {
    opacity: 0;
    transition: opacity 200ms ease-out;
  }
  .expanded .content {
    opacity: 1;
    transition-delay: 120ms;
  }
  @media (prefers-reduced-motion: reduce) {
    .pill {
      transition-duration: 1ms;
    }
  }
</style>
