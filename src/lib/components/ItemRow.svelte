<script lang="ts">
  import type { ItemView } from "../api/types";
  import { api } from "../api/commands";
  import { clock, ruleSummary, when } from "../format";
  import { app } from "../stores/app.svelte";
  import Icon from "./Icon.svelte";
  import SnoozeMenu from "./SnoozeMenu.svelte";
  import TagDot from "./TagDot.svelte";
  import ItemActions from "./ItemActions.svelte";
  import ReminderCountdown from "./ReminderCountdown.svelte";
  import { haptic } from "../motion";
  let menu: ItemActions;

  let { item, onedit }: { item: ItemView; onedit: (item: ItemView) => void } = $props();

  const HELD = { quiet: "held for quiet hours", mute: "muted", handoff: "waiting on your phone" } as const;

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
        return `Due ${when(s.occurrence, now)}`;
      case "upcoming":
        return when(s.at, now);
      case "idle":
        return "Done";
      case "unscheduled":
        return "Can't read this item's schedule (made by a newer Dun?)";
    }
  });

  const occurrence = $derived(item.status.kind === "due" ? item.status.occurrence : null);

  /**
   * Shows the check from the tap until the row has left. A recurring reminder
   * (or a failed Done) stays in the list, so the check clears again after.
   */
  let completing = $state(false);

  async function done() {
    if (completing) return;
    completing = true;
    haptic(10);
    await app.run(() => api.done(item.id, occurrence));
    setTimeout(() => (completing = false), 500);
  }
</script>

<div class="row" role="group" class:ringing class:completing class:overdue={item.status.kind === "due" && !ringing} oncontextmenu={(e) => menu.openAt(e)}>
  <button
    class="done"
    onclick={done}
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
  <div class="countdown-slot">
    <ReminderCountdown title={item.title} status={item.status} {now} />
  </div>

  <div class="actions">
  {#if item.status.kind === "due"}
    <SnoozeMenu onsnooze={(m) => app.run(() => api.snooze(item.id, m, occurrence))} />
  {/if}
  <ItemActions bind:this={menu} {item} {onedit} />
  </div>
</div>

<style>
  .row {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) max-content auto;
    align-items: center;
    gap: 0.6rem;
    padding: 0.55rem 0.6rem 0.55rem 0.7rem;
    border-radius: var(--radius);
    background: var(--bg-raised);
    border: 1px solid var(--border);
    transition: background-color var(--dur-slow) ease-out, border-color var(--dur-slow) ease-out;
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
    transition: background-color var(--dur-fast) ease-out, border-color var(--dur-fast) ease-out, color var(--dur-fast) ease-out, transform var(--dur-fast) var(--ease-out);
  }
  .ringing .done {
    border-color: var(--ringing);
  }
  @media (hover: hover) {
    .done:hover {
      border-color: var(--ok);
      color: var(--ok);
    }
    .row:hover { border-color: color-mix(in srgb, var(--border) 60%, var(--fg-faint)); }
  }
  .done:active { transform: scale(0.9); }
  .completing .done {
    border-color: var(--ok);
    background: var(--ok);
    color: var(--bg-raised);
    animation: check-in var(--dur-slow) var(--ease-out);
  }
  .completing .title {
    color: var(--fg-muted);
    text-decoration: line-through;
    text-decoration-color: var(--fg-faint);
  }
  @keyframes check-in {
    from { transform: scale(0.75); }
    60% { transform: scale(1.12); }
    to { transform: scale(1); }
  }
  .main {
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
  .countdown-slot { justify-self: end; }
  .actions { display: flex; align-items: center; justify-content: flex-end; min-width: 9.5rem; gap: 0.3rem; }
  @media (max-width: 600px), (min-width: 760px) and (max-aspect-ratio: 3/4) {
    .row { grid-template-columns: auto minmax(0, 1fr) max-content; row-gap: 0.4rem; column-gap: 0.5rem; }
    .done { grid-column: 1; grid-row: 1; }
    .main { grid-column: 2 / -1; grid-row: 1; }
    .countdown-slot { grid-column: 3; grid-row: 2; }
    .actions { grid-column: 1 / 3; grid-row: 2; justify-content: flex-start; min-width: 0; }
    .actions :global(.btn) { padding-inline: 0.55rem; }
    .actions :global(.btn > svg) { display: none; }
  }
  @media (pointer: coarse) { .done { width: 44px; height: 44px; } }
</style>
