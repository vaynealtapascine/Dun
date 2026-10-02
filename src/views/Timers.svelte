<script lang="ts">
  import type { ItemView, Preset } from "../lib/api/types";
  import { api } from "../lib/api/commands";
  import { formatShort } from "../lib/duration";
  import { groupTimers } from "../lib/grouping";
  import { app } from "../lib/stores/app.svelte";
  import { reorder, rowIn, rowOut } from "../lib/motion";
  import { wheelScrollsX } from "../lib/gestures";
  import Icon from "../lib/components/Icon.svelte";
  import SwipeToArchive from "../lib/components/SwipeToArchive.svelte";
  import TimerCard from "../lib/components/TimerCard.svelte";
  import TimerStart from "../lib/components/TimerStart.svelte";
  import PresetForm from "./PresetForm.svelte";

  let { items, onedit }: { items: ItemView[]; onedit: (item: ItemView) => void } = $props();

  const buckets = $derived(groupTimers(items));
  const active = $derived(buckets.active);
  const presets = $derived([...(app.snapshot?.presets ?? [])].filter((p) => !p.deleted).sort((a, b) => a.order - b.order));

  let presetOpen = $state(false);
  let editing = $state<Preset | null>(null);
  let starting = $state<string[]>([]);
  let managingPresets = $state(false);

  function editPreset(preset: Preset | null) {
    editing = preset;
    presetOpen = true;
  }

  async function startPreset(preset: Preset) {
    if (starting.includes(preset.id)) return;
    starting = [...starting, preset.id];
    try {
      const id = await app.run(() => api.startPreset(preset.id));
      if (id) app.notify(`Started “${preset.name}”`);
    } finally {
      starting = starting.filter((id) => id !== preset.id);
    }
  }
</script>

<div class="timer-workspace">
  <!-- Always present, so the last timer can leave with the same motion as the rest. -->
    <div class="timer-list" role="list" aria-label="Active timers">
      {#each active as item (item.id)}
        <div class="row-slot" role="listitem" animate:reorder in:rowIn out:rowOut>
          <SwipeToArchive label={item.title} onarchive={() => app.archive(item)}>
            <TimerCard {item} {onedit} />
          </SwipeToArchive>
        </div>
      {/each}
    </div>

  <TimerStart {managingPresets} onmanagepresets={() => managingPresets = !managingPresets}>
    <div class="presets" role="group" aria-label="Timer presets" use:wheelScrollsX>
      {#each presets as preset (preset.id)}
        <div class="preset">
          <button
            class="preset-start"
            onclick={() => startPreset(preset)}
            disabled={starting.includes(preset.id)}
            aria-label="Start {preset.name}, {formatShort(preset.durationMs)}"
          >
            <span class="preset-name">{preset.name}</span>
            <span class="preset-duration">{formatShort(preset.durationMs)}</span>
          </button>
        </div>
      {/each}
    </div>
    {#if managingPresets || presets.length === 0}
      <div class="preset-management" role="group" aria-label="Manage presets">
        {#each presets as preset (preset.id)}<button class="btn" onclick={() => editPreset(preset)}>Edit {preset.name}</button>{/each}
        <button class="btn" onclick={() => editPreset(null)}><Icon name="plus" size={16} /> Add preset</button>
      </div>
    {/if}
  </TimerStart>

  {#if buckets.idle.length > 0}
    <section class="ready" in:rowIn out:rowOut>
      <h2>Ready to start</h2>
      <div class="timer-list" role="list" aria-label="Ready timers">
        {#each buckets.idle as item (item.id)}
          <div class="row-slot" role="listitem" animate:reorder in:rowIn out:rowOut>
            <SwipeToArchive label={item.title} onarchive={() => app.archive(item)}>
              <TimerCard {item} {onedit} />
            </SwipeToArchive>
          </div>
        {/each}
      </div>
    </section>
  {/if}
</div>

<PresetForm bind:open={presetOpen} preset={editing} />

<style>
  .timer-workspace,
  .timer-list {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 0.5rem;
    min-width: 0;
  }
  .presets {
    display: flex;
    flex-wrap: nowrap;
    overflow-x: auto;
    overscroll-behavior-x: contain;
    scroll-snap-type: x proximity;
    padding-bottom: 0.1rem;
    gap: 0.45rem;
    min-width: 0;
  }
  .timer-list { gap: 0.5625rem; }
  .timer-list:empty { display: none; }
  .preset {
    scroll-snap-align: start;
    display: flex;
    flex: 1 0 calc((100% - 0.9rem) / 3);
    min-width: 0;
    background: var(--bg-sunken);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .preset-start {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    flex: 1;
    min-width: 0;
    min-height: max(44px, 2.1rem);
    padding: 0.45rem 0.55rem;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    font-weight: 700;
    font-size: 0.75rem;
    cursor: pointer;
  }
  .preset-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .preset-duration {
    flex: none;
    font-variant-numeric: tabular-nums;
  }
  .preset-start { transition: background-color var(--dur-fast) ease-out, transform var(--dur-fast) var(--ease-out); }
  @media (hover: hover) { .preset-start:hover { background: color-mix(in srgb, var(--fg) 7%, transparent); } }
  .preset-start:active:not(:disabled) { background: color-mix(in srgb, var(--fg) 12%, transparent); transform: scale(0.97); }
  .preset-start:disabled { opacity: 0.55; cursor: wait; }
  .ready {
    display: grid;
    gap: 0.5rem;
    margin-top: 0.6rem;
  }
  .preset-management { display: flex; flex-wrap: wrap; gap: 0.5rem; margin-top: 0.5rem; }
  h2 {
    margin: 0;
    font-family: var(--font-ui);
    font-size: 1.3rem;
    line-height: 1.2;
    font-weight: 700;
  }
</style>
