<script lang="ts">
  import type { ItemView, Preset } from "../lib/api/types";
  import { api } from "../lib/api/commands";
  import { formatShort } from "../lib/duration";
  import { groupTimers } from "../lib/grouping";
  import { app } from "../lib/stores/app.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import SwipeToDelete from "../lib/components/SwipeToDelete.svelte";
  import TimerCard from "../lib/components/TimerCard.svelte";
  import PresetForm from "./PresetForm.svelte";

  let { items, onedit }: { items: ItemView[]; onedit: (item: ItemView) => void } = $props();

  const buckets = $derived(groupTimers(items));
  const active = $derived([...buckets.ringing, ...buckets.running, ...buckets.paused]);
  const presets = $derived([...(app.snapshot?.presets ?? [])].sort((a, b) => a.order - b.order));

  let presetOpen = $state(false);
  let editing = $state<Preset | null>(null);
</script>

<section>
  <h3>Presets</h3>
  <div class="presets">
    {#each presets as p (p.id)}
      <div class="preset card">
        <button class="start" onclick={() => app.run(() => api.startPreset(p.id))} title="Start {p.name}">
          <span class="name">{p.name}</span>
          <span class="muted">{formatShort(p.durationMs)}</span>
        </button>
        <button
          class="icon-btn edit"
          aria-label="Edit {p.name}"
          onclick={() => {
            editing = p;
            presetOpen = true;
          }}><Icon name="edit" size={15} /></button
        >
      </div>
    {/each}
    <button
      class="preset add"
      onclick={() => {
        editing = null;
        presetOpen = true;
      }}><Icon name="plus" size={16} /> Preset</button
    >
  </div>
</section>

{#if active.length > 0}
  <section>
    <h3>Running</h3>
    <div class="list">
      {#each active as item (item.id)}
        <SwipeToDelete label={item.title} ondelete={() => app.run(() => api.deleteItem(item.id))}>
          <TimerCard {item} {onedit} />
        </SwipeToDelete>
      {/each}
    </div>
  </section>
{/if}

{#if buckets.idle.length > 0}
  <section>
    <h3>Stopped</h3>
    <div class="list">
      {#each buckets.idle as item (item.id)}
        <SwipeToDelete label={item.title} ondelete={() => app.run(() => api.deleteItem(item.id))}>
          <TimerCard {item} {onedit} />
        </SwipeToDelete>
      {/each}
    </div>
  </section>
{/if}

{#if active.length === 0 && buckets.idle.length === 0}
  <p class="muted empty">No timers yet. Start a preset or add one with +.</p>
{/if}

<PresetForm bind:open={presetOpen} preset={editing} />

<style>
  section {
    display: grid;
    gap: 0.45rem;
  }
  section + section {
    margin-top: 1rem;
  }
  h3 {
    margin: 0;
    padding: 0 0.2rem;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-muted);
  }
  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
  .preset {
    display: flex;
    align-items: center;
    border-radius: 999px;
    padding: 0 0.15rem 0 0;
  }
  .start {
    display: flex;
    gap: 0.4rem;
    align-items: baseline;
    padding: 0.45rem 0.4rem 0.45rem 0.9rem;
    border: none;
    background: none;
    cursor: pointer;
  }
  .name {
    font-weight: 600;
  }
  .edit {
    width: 1.9rem;
    height: 1.9rem;
  }
  .add {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.45rem 0.9rem;
    border: 1px dashed var(--fg-faint);
    background: transparent;
    color: var(--fg-muted);
    cursor: pointer;
  }
  .list {
    display: grid;
    gap: 0.45rem;
  }
  .empty {
    text-align: center;
    padding: 2.5rem 1rem;
  }
</style>
