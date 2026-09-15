<script lang="ts">
  import { displayAccelerator, fromKeyEvent, hasModifier } from "../accelerator";

  let {
    value,
    defaultValue,
    onchange,
  }: { value: string; defaultValue: string; onchange: (accelerator: string) => void } = $props();

  let recording = $state(false);
  let hint = $state<string | null>(null);

  function onkeydown(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.code === "Escape" && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) {
      recording = false;
      hint = null;
      return;
    }
    const accelerator = fromKeyEvent(e);
    if (!accelerator) return;
    if (!hasModifier(accelerator)) {
      hint = "Hold Ctrl, Alt, Shift or Win too";
      return;
    }
    recording = false;
    hint = null;
    onchange(accelerator);
  }
</script>

<div class="hotkey">
  <button
    type="button"
    class="keys"
    class:recording
    aria-label={recording ? "Press the new shortcut, or Escape to cancel" : `Quick-add shortcut: ${displayAccelerator(value)}. Change`}
    onclick={() => {
      recording = !recording;
      hint = null;
    }}
    {onkeydown}
    onblur={() => (recording = false)}
  >
    {recording ? "Press keys…" : displayAccelerator(value)}
  </button>
  {#if value !== defaultValue}
    <button type="button" class="btn btn-quiet small" onclick={() => onchange(defaultValue)}>Reset</button>
  {/if}
  {#if value}
    <button type="button" class="btn btn-quiet small" onclick={() => onchange("")}>Turn off</button>
  {/if}
</div>
{#if hint}<p class="hint">{hint}</p>{/if}

<style>
  .hotkey {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem;
  }
  .keys {
    min-width: 9rem;
    min-height: 2.4rem;
    padding: 0.4rem 0.8rem;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--bg-sunken);
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 0.9rem;
    cursor: pointer;
  }
  .keys.recording {
    border-color: var(--focus);
    color: var(--focus);
  }
  .small {
    min-height: 2rem;
    font-size: 0.85rem;
  }
  .hint {
    margin: 0;
    font-size: 0.83rem;
    color: var(--overdue);
  }
</style>
