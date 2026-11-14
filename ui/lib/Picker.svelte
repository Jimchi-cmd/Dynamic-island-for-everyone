<script lang="ts" module>
  export interface PickerOption {
    value: string;
    label: string;
    /** A short grey note after the label. */
    hint?: string;
    /** Draw this option's label in a font of that name (font lists). */
    font?: string;
  }
</script>

<script lang="ts">
  // A dropdown that matches the rest of Settings (the system's own <select>
  // list is a plain white box on Windows). Keyboard: arrows, Home, End,
  // Enter, Escape, and typing to search when `searchable`.
  import { tick } from 'svelte';

  let {
    options,
    value,
    label,
    disabled = false,
    searchable = false,
    onchange,
  }: {
    options: PickerOption[];
    value: string;
    label: string;
    disabled?: boolean;
    searchable?: boolean;
    onchange: (value: string) => void;
  } = $props();

  let open = $state(false);
  let query = $state('');
  let active = $state(0);
  let root: HTMLDivElement | null = $state(null);
  let list: HTMLUListElement | null = $state(null);
  let search: HTMLInputElement | null = $state(null);
  let button: HTMLButtonElement | null = $state(null);
  const id = `picker-${Math.random().toString(36).slice(2, 9)}`;

  const chosen = $derived(options.find((o) => o.value === value));
  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? options.filter((o) => o.label.toLowerCase().includes(q)) : options;
  });

  async function show() {
    if (disabled) return;
    open = true;
    query = '';
    active = Math.max(0, options.findIndex((o) => o.value === value));
    await tick();
    (searchable ? search : list)?.focus();
    scrollToActive();
  }

  function close(refocus = true) {
    open = false;
    if (refocus) button?.focus();
  }

  function pick(o: PickerOption | undefined) {
    if (!o) return;
    close();
    if (o.value !== value) onchange(o.value);
  }

  function scrollToActive() {
    list?.querySelector<HTMLElement>(`[data-i="${active}"]`)?.scrollIntoView({ block: 'nearest' });
  }

  function keys(e: KeyboardEvent) {
    const last = shown.length - 1;
    const move = (to: number) => {
      e.preventDefault();
      active = Math.min(Math.max(to, 0), Math.max(last, 0));
      scrollToActive();
    };
    if (e.key === 'ArrowDown') move(active + 1);
    else if (e.key === 'ArrowUp') move(active - 1);
    else if (e.key === 'Home' && !searchable) move(0);
    else if (e.key === 'End' && !searchable) move(last);
    else if (e.key === 'PageDown') move(active + 8);
    else if (e.key === 'PageUp') move(active - 8);
    else if (e.key === 'Enter') {
      e.preventDefault();
      pick(shown[active]);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      close();
    } else if (e.key === 'Tab') close(false);
  }

  function buttonKeys(e: KeyboardEvent) {
    if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(e.key)) {
      e.preventDefault();
      void show();
    }
  }

  // A click anywhere else closes the list.
  function outside(e: MouseEvent) {
    if (open && root && !root.contains(e.target as Node)) close(false);
  }
</script>

<svelte:window onmousedown={outside} />

<div class="picker" class:open bind:this={root}>
  <button
    bind:this={button}
    type="button"
    class="field"
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-label={`${label}: ${chosen?.label ?? value}`}
    {disabled}
    onclick={() => (open ? close() : show())}
    onkeydown={buttonKeys}
  >
    <span class="current" style:font-family={chosen?.font ? `"${chosen.font}", var(--ui-font)` : null}>
      {chosen?.label ?? value}
    </span>
    <svg class="chevron" viewBox="0 0 12 12" aria-hidden="true"><path d="M3 4.5 6 7.5 9 4.5" /></svg>
  </button>
  {#if open}
    <div class="pop" role="presentation">
      {#if searchable}
        <input
          bind:this={search}
          bind:value={query}
          class="search"
          placeholder="Search…"
          aria-label={`Search ${label}`}
          aria-controls={id}
          aria-activedescendant={shown[active] ? `${id}-${active}` : undefined}
          oninput={() => (active = 0)}
          onkeydown={keys}
        />
      {/if}
      <ul
        bind:this={list}
        {id}
        role="listbox"
        aria-label={label}
        tabindex={searchable ? -1 : 0}
        aria-activedescendant={shown[active] ? `${id}-${active}` : undefined}
        onkeydown={keys}
      >
        {#each shown as o, i (o.value)}
          <li
            id={`${id}-${i}`}
            data-i={i}
            role="option"
            aria-selected={o.value === value}
            class:active={i === active}
            onmousemove={() => (active = i)}
            onclick={() => pick(o)}
            onkeydown={keys}
          >
            <svg class="check" viewBox="0 0 12 12" aria-hidden="true"
              ><path d="M2.5 6.4 5 8.8 9.6 3.4" /></svg
            >
            <span class="label" style:font-family={o.font ? `"${o.font}", var(--ui-font)` : null}>{o.label}</span>
            {#if o.hint}<span class="hint">{o.hint}</span>{/if}
          </li>
        {:else}
          <li class="empty" role="presentation">Nothing matches</li>
        {/each}
      </ul>
    </div>
  {/if}
</div>

<style>
  .picker {
    position: relative;
    min-width: 200px;
  }
  .field {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
    padding: 6px 10px 6px 12px;
    border-radius: 8px;
    background: var(--field);
    box-shadow: none;
    font-weight: 500;
  }
  .field:hover:not(:disabled) {
    filter: none;
    background: color-mix(in srgb, var(--field) 100%, var(--hover));
  }
  .open .field {
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .current {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chevron {
    width: 12px;
    height: 12px;
    flex: none;
    fill: none;
    stroke: var(--muted);
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
    transition: transform 0.15s ease;
  }
  .open .chevron {
    transform: rotate(180deg);
  }
  .pop {
    position: absolute;
    z-index: 30;
    top: calc(100% + 6px);
    right: 0;
    min-width: 100%;
    max-width: 340px;
    padding: 5px;
    border-radius: 11px;
    background: var(--panel);
    box-shadow:
      0 0 0 0.5px var(--line),
      0 10px 30px rgba(0, 0, 0, 0.22),
      0 2px 6px rgba(0, 0, 0, 0.12);
    animation: drop 0.12s ease-out;
  }
  @keyframes drop {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.98);
    }
  }
  .search {
    width: 100%;
    margin-bottom: 5px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 280px;
    overflow-y: auto;
    outline: none;
  }
  li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px 7px 8px;
    border-radius: 7px;
    cursor: pointer;
    white-space: nowrap;
    color: var(--text);
  }
  li.active {
    background: var(--accent);
    color: var(--accent-text);
  }
  li.active .hint {
    color: inherit;
    opacity: 0.85;
  }
  .check {
    width: 13px;
    height: 13px;
    flex: none;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
    visibility: hidden;
  }
  li[aria-selected='true'] .check {
    visibility: visible;
  }
  .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hint {
    color: var(--muted);
    font-size: 12px;
  }
  .empty {
    color: var(--muted);
    cursor: default;
  }
</style>
