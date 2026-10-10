<script lang="ts">
  import type { Countdown } from "../lib/api/types";
  import { sortCountdowns } from "../lib/countdown";
  import { app } from "../lib/stores/app.svelte";
  import { ms, reorder, rowIn, rowOut } from "../lib/motion";
  import { fade } from "svelte/transition";
  import CountdownCard from "../lib/components/CountdownCard.svelte";

  let { query = "", onedit }: { query?: string; onedit: (c: Countdown) => void } = $props();

  const entries = $derived(
    sortCountdowns(
      (app.snapshot?.countdowns ?? []).filter((c) => `${c.title} ${c.notes}`.toLowerCase().includes(query.toLowerCase())),
      app.now,
    ),
  );
</script>

{#if entries.length === 0}
  <div class="empty" in:fade={{ duration: ms(200) }}>
    {#if query}
      <p class="big">No matching countdowns.</p>
    {:else}
      <p class="big">Count the days to anything.</p>
      <p class="muted">Birthdays, trips, deadlines, or how long since something started. Add one with the + button.</p>
    {/if}
  </div>
{:else}
  <div class="list" role="list">
    {#each entries as { countdown, reading } (countdown.id)}
      <div role="listitem" animate:reorder in:rowIn out:rowOut>
        <CountdownCard {countdown} {reading} {onedit} />
      </div>
    {/each}
  </div>
{/if}

<style>
  .list {
    display: grid;
    gap: 0.5rem;
  }
  .empty {
    text-align: center;
    padding: 4rem 1rem;
  }
  .big {
    font-size: 1.1rem;
    font-weight: 600;
    margin-bottom: 0.2rem;
  }
</style>
