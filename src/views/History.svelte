<script lang="ts">
  import type { HistoryRow } from "../lib/api/types";
  import { api, errorText } from "../lib/api/commands";
  import { clock, dayLabel, when } from "../lib/format";
  import { app } from "../lib/stores/app.svelte";
  import Icon from "../lib/components/Icon.svelte";

  let rows = $state<HistoryRow[]>([]);
  let error = $state<string | null>(null);

  async function load() {
    try {
      rows = await api.history(null, null, 300);
      error = null;
    } catch (e) {
      error = errorText(e);
    }
  }

  // Reload whenever anything changes (a Done from a toast or the phone adds rows).
  $effect(() => {
    void app.snapshot;
    load();
  });

  const undone = $derived(new Set(rows.filter((r) => r.kind === "undo" && r.refId).map((r) => r.refId)));

  const days = $derived.by(() => {
    const out: { label: string; rows: HistoryRow[] }[] = [];
    for (const row of rows.filter((r) => r.kind !== "undo")) {
      const label = dayLabel(row.at, app.now);
      const last = out.at(-1);
      if (last?.label === label) last.rows.push(row);
      else out.push({ label, rows: [row] });
    }
    return out;
  });

  const exists = (itemId: string) => app.snapshot?.items.some((i) => i.id === itemId) ?? false;
</script>

{#if error}<p class="error">{error}</p>{/if}

{#if days.length === 0}
  <p class="muted empty">Finished reminders and timers show up here.</p>
{/if}

{#each days as day (day.label)}
  <section>
    <h3>{day.label}</h3>
    <div class="list">
      {#each day.rows as row (row.id)}
        <div class="row card" class:undone={undone.has(row.id)}>
          <div class="main">
            <span class="title">{row.title || "Untitled"}</span>
            <span class="muted sub">
              {#if row.kind === "restart"}
                Restarted {clock(row.at)}
              {:else}
                Done {clock(row.at)}{#if row.occurrence && Math.abs(row.at - row.occurrence) > 60_000}
                  · was due {when(row.occurrence, row.at)}{/if}{#if row.snoozeCount > 0}
                  · snoozed {row.snoozeCount}×{/if}{#if undone.has(row.id)} · undone{/if}
              {/if}
            </span>
          </div>
          {#if row.kind === "done" && !undone.has(row.id) && exists(row.itemId)}
            <button class="btn" onclick={() => app.run(() => api.undo(row.id))} title="Bring it back">
              <Icon name="undo" size={15} /> Undo
            </button>
          {/if}
        </div>
      {/each}
    </div>
  </section>
{/each}

<style>
  section {
    display: grid;
    gap: 0.4rem;
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
  .list {
    display: grid;
    gap: 0.35rem;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.5rem 0.6rem 0.5rem 0.8rem;
  }
  .row.undone {
    opacity: 0.6;
  }
  .main {
    flex: 1;
    min-width: 0;
    display: grid;
  }
  .title {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    font-size: 0.83rem;
  }
  .empty {
    text-align: center;
    padding: 3rem 1rem;
  }
  .error {
    color: var(--ringing);
  }
</style>
