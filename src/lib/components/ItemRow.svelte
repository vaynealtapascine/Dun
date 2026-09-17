<script lang="ts">
  import type { ItemView } from "../api/types";
  import { api } from "../api/commands";
  import { clock, relative, ruleSummary, when } from "../format";
  import { app } from "../stores/app.svelte";
  import Icon from "./Icon.svelte";
  import SnoozeMenu from "./SnoozeMenu.svelte";
  import TagDot from "./TagDot.svelte";

  let { item, onedit }: { item: ItemView; onedit: (item: ItemView) => void } = $props();

  const HELD = { quiet: "held for quiet hours", mute: "muted", handoff: "ringing on your phone" } as const;

  const now = $derived(app.now);
  const tag = $derived(app.tag(item.tag));
  const ringing = $derived(item.status.kind === "due" && !item.status.snoozed && item.ring?.held == null);

  const subtitle = $derived.by(() => {
    const s = item.status;
    switch (s.kind) {
      case "due":
        if (s.snoozed) return `Snoozed until ${clock(s.ringsAt)}`;
        if (s.firstMissed != null && s.missedCount > 1) {
          return `Missed ${s.missedCount}× since ${when(s.firstMissed, now)}`;
        }
        if (item.ring?.held) return `Due ${when(s.occurrence, now)} · ${HELD[item.ring.held]}`;
        return `Due ${when(s.occurrence, now)} · ${relative(s.occurrence, now)}`;
      case "upcoming":
        return `${when(s.at, now)} · ${relative(s.at, now)}`;
      case "idle":
        return "Done";
      case "unscheduled":
        return "Can't read this item's schedule (made by a newer Dun?)";
    }
  });

  const occurrence = $derived(item.status.kind === "due" ? item.status.occurrence : null);
</script>

<div class="row" class:ringing class:overdue={item.status.kind === "due" && !ringing}>
  <button
    class="done"
    onclick={() => app.run(() => api.done(item.id, occurrence))}
    aria-label="Mark “{item.title}” done"
    title="Done"
  >
    <Icon name="check" size={16} />
  </button>

  <button class="main" onclick={() => onedit(item)}>
    <span class="title">{item.title}</span>
    <span class="sub">
      {#if tag}<TagDot color={tag.color} />{/if}
      <span>{subtitle}</span>
    </span>
    {#if item.schedule?.kind === "recurring"}
      <span class="sub repeat"><Icon name="repeat" size={13} /> {ruleSummary(item.schedule.recurrence.rule)}</span>
    {/if}
  </button>

  {#if item.status.kind === "due"}
    <SnoozeMenu onsnooze={(m) => app.run(() => api.snooze(item.id, m, occurrence))} />
  {/if}
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.55rem 0.6rem 0.55rem 0.7rem;
    border-radius: var(--radius);
    background: var(--bg-raised);
    border: 1px solid var(--border);
  }
  .row.ringing {
    background: var(--ringing-bg);
    border-color: color-mix(in srgb, var(--ringing) 35%, transparent);
  }
  .row.overdue {
    background: var(--overdue-bg);
    border-color: color-mix(in srgb, var(--overdue) 30%, transparent);
  }
  .done {
    flex: none;
    display: grid;
    place-items: center;
    width: 1.75rem;
    height: 1.75rem;
    border-radius: 999px;
    border: 2px solid var(--fg-faint);
    background: transparent;
    color: transparent;
    cursor: pointer;
    transition: all 0.12s;
  }
  .ringing .done {
    border-color: var(--ringing);
  }
  .done:hover {
    border-color: var(--ok);
    background: var(--ok);
    color: #fff;
  }
  .main {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 0.1rem;
    text-align: left;
    background: none;
    border: none;
    padding: 0.1rem 0;
    cursor: pointer;
  }
  .title {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.83rem;
    color: var(--fg-muted);
    min-width: 0;
  }
  .sub > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ringing .sub:first-of-type {
    color: var(--ringing);
  }
  .overdue .sub:first-of-type {
    color: var(--overdue);
  }
  .repeat {
    font-size: 0.78rem;
  }
</style>
