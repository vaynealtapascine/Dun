<script lang="ts">
  import type { ItemView } from "../api/types";
  import { api } from "../api/commands";
  import { formatCountdown } from "../duration";
  import { app } from "../stores/app.svelte";
  import Icon from "./Icon.svelte";
  import SnoozeMenu from "./SnoozeMenu.svelte";
  import TagDot from "./TagDot.svelte";

  let { item, onedit }: { item: ItemView; onedit: (item: ItemView) => void } = $props();

  const duration = $derived(item.schedule?.kind === "timer" ? item.schedule.durationMs : 0);
  const ringing = $derived(item.status.kind === "due");
  const snoozed = $derived(item.status.kind === "due" && item.status.snoozed);
  const remaining = $derived.by(() => {
    const t = item.timer;
    if (t.state === "running") return t.endAt - app.now;
    if (t.state === "paused") return t.remainingMs;
    return duration;
  });
  const fraction = $derived(duration > 0 ? Math.min(1, Math.max(0, remaining / duration)) : 0);
  const tag = $derived(app.tag(item.tag));

  const R = 34;
  const C = 2 * Math.PI * R;
  const occurrence = $derived(item.status.kind === "due" ? item.status.occurrence : null);

  const act = (action: "start" | "pause" | "resume" | "reset") => app.run(() => api.timer(item.id, action));
</script>

<article class="card timer" class:ringing class:paused={item.timer.state === "paused"}>
  <div class="ring" aria-hidden="true">
    <svg viewBox="0 0 80 80">
      <circle cx="40" cy="40" r={R} class="track" />
      <circle
        cx="40"
        cy="40"
        r={R}
        class="progress"
        stroke-dasharray={C}
        stroke-dashoffset={C * (1 - fraction)}
        transform="rotate(-90 40 40)"
      />
    </svg>
  </div>

  <div class="info">
    <button class="title" onclick={() => onedit(item)}>
      {#if tag}<TagDot color={tag.color} />{/if}
      {item.title}
    </button>
    <div class="time" aria-live={ringing ? "polite" : "off"}>
      {#if ringing && !snoozed}
        Time's up · {formatCountdown(-remaining)} ago
      {:else if snoozed && item.status.kind === "due"}
        Snoozed · {formatCountdown(item.status.ringsAt - app.now)}
      {:else}
        {formatCountdown(remaining)}
      {/if}
    </div>
  </div>

  <div class="actions">
    {#if ringing}
      <button class="btn btn-primary" onclick={() => app.run(() => api.done(item.id, occurrence))}>
        <Icon name="check" size={16} /> Done
      </button>
      <SnoozeMenu onsnooze={(m) => app.run(() => api.snooze(item.id, m, occurrence))} />
      <button class="icon-btn" aria-label="Restart" title="Restart" onclick={() => act("start")}><Icon name="reset" /></button>
    {:else if item.timer.state === "running"}
      <button class="icon-btn" aria-label="Pause" title="Pause" onclick={() => act("pause")}><Icon name="pause" /></button>
      <button class="icon-btn" aria-label="Reset" title="Reset" onclick={() => act("reset")}><Icon name="reset" /></button>
    {:else if item.timer.state === "paused"}
      <button class="icon-btn" aria-label="Resume" title="Resume" onclick={() => act("resume")}><Icon name="play" /></button>
      <button class="icon-btn" aria-label="Reset" title="Reset" onclick={() => act("reset")}><Icon name="reset" /></button>
    {:else}
      <button class="btn" onclick={() => act("start")}><Icon name="play" size={14} /> Start</button>
    {/if}
  </div>
</article>

<style>
  .timer {
    display: flex;
    align-items: center;
    gap: 0.8rem;
    padding: 0.6rem 0.7rem;
  }
  .timer.ringing {
    background: var(--ringing-bg);
    border-color: color-mix(in srgb, var(--ringing) 35%, transparent);
  }
  .ring {
    flex: none;
    width: 3.4rem;
    height: 3.4rem;
  }
  svg {
    width: 100%;
    height: 100%;
  }
  .track {
    fill: none;
    stroke: var(--bg-sunken);
    stroke-width: 7;
  }
  .progress {
    fill: none;
    stroke: var(--accent);
    stroke-width: 7;
    stroke-linecap: round;
    transition: stroke-dashoffset 0.9s linear;
  }
  .paused .progress {
    stroke: var(--fg-faint);
  }
  .info {
    flex: 1;
    min-width: 0;
    display: grid;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0;
    border: none;
    background: none;
    font-weight: 600;
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .time {
    font-size: 1.35rem;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.01em;
  }
  .ringing .time {
    color: var(--ringing);
    font-size: 1.05rem;
    font-weight: 600;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }
</style>
