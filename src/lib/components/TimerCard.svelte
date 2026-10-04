<script lang="ts">
  import type { ItemView } from "../api/types";
  import { api } from "../api/commands";
  import { formatCountdown } from "../duration";
  import { app } from "../stores/app.svelte";
  import SnoozeMenu from "./SnoozeMenu.svelte";
  import TagDot from "./TagDot.svelte";
  import ItemActions from "./ItemActions.svelte";
  import SourceLink from "./SourceLink.svelte";
  import { easeOut, ms } from "../motion";

  let { item, onedit }: { item: ItemView; onedit: (item: ItemView) => void } = $props();
  let menu: ItemActions | undefined = $state();

  const duration = $derived(item.schedule?.kind === "timer" ? item.schedule.durationMs : 0);
  const canStart = $derived(Number.isFinite(duration) && duration >= 1000);
  const due = $derived(item.status.kind === "due");
  const snoozed = $derived(item.status.kind === "due" && item.status.snoozed);
  const held = $derived(item.ring?.held ?? null);
  const ringing = $derived(due && !snoozed && held === null);
  const occurrence = $derived(item.status.kind === "due" ? item.status.occurrence : null);
  const tag = $derived(app.tag(item.tag));

  const remaining = $derived.by(() => {
    const timer = item.timer;
    if (timer.state === "running") return Math.max(0, timer.endAt - app.now);
    if (timer.state === "paused") return Math.max(0, timer.remainingMs);
    return Math.max(0, duration);
  });
  const fraction = $derived(duration > 0 ? Math.min(1, Math.max(0, remaining / duration)) : 0);
  const elapsed = $derived(item.status.kind === "due" ? Math.max(0, app.now - item.status.occurrence) : 0);
  // A snoozed face counts down to the next alert; a finished one keeps
  // counting up from when it went off, so it says how long it has waited.
  const display = $derived(formatCountdown(
    due ? (snoozed && item.status.kind === "due" ? Math.max(0, item.status.ringsAt - app.now) : -elapsed) : remaining,
  ));
  const overdue = $derived(display.startsWith("-"));
  const unsigned = $derived(overdue ? display.slice(1) : display);
  const dayBreak = $derived(unsigned.indexOf("d "));
  const days = $derived(dayBreak >= 0 ? unsigned.slice(0, dayBreak + 1) : null);
  const clock = $derived(dayBreak >= 0 ? unsigned.slice(dayBreak + 2) : unsigned);
  const stateLabel = $derived.by(() => {
    if (snoozed) return "Snoozed";
    if (due) {
      if (held === "quiet") return "Quiet hours";
      if (held === "mute") return "Alerts muted";
      if (held === "handoff") return "Waiting on phone";
      return "Time’s up";
    }
    if (item.timer.state === "running") return "Running";
    if (item.timer.state === "paused") return "Paused";
    return canStart ? "Ready" : "Set a duration";
  });

  const R = 44;
  const C = 2 * Math.PI * R;
  /** The finished face settles in, so the change of state reads as one event. */
  function faceIn(_node: Element) {
    return {
      duration: ms(320),
      easing: easeOut,
      css: (t: number, u: number) => `opacity: ${t}; transform: scale(${1 - 0.06 * u});`,
    };
  }

  const act = (action: "start" | "pause" | "resume" | "reset") => app.run(() => api.timer(item.id, action));
</script>

<div class="sr-only" role="status" aria-live="polite" aria-atomic="true">{item.title}: {stateLabel}</div>

<article class="card timer" class:due class:ringing class:paused={item.timer.state === "paused"}
  class:running={!due && item.timer.state === "running"} oncontextmenu={(e) => menu?.openAt(e)}>
  {#if due}
    <div class="alarm-header">
      <div class="alarm-heading">
        <button class="title" title="Edit {item.title}" onclick={() => onedit(item)}>
          {#if tag}<TagDot color={tag.color} />{/if}
          <span>{item.title}</span>
        </button>
        <SourceLink notes={item.notes} />
      </div>
      <div class="more"><ItemActions bind:this={menu} {item} {onedit} /></div>
    </div>
    <div class="alarm-display" in:faceIn>
      <div class="alarm-time" class:long={clock.length > 5} role="timer" aria-live="off"
        aria-label={snoozed ? `${display} until alert resumes` : overdue ? `Finished ${unsigned} ago` : "Timer finished"}>
        {#if days}<span class="days">{overdue ? "−" : ""}{days}</span>{/if}
        <span>{#if overdue && !days}<span class="sign">−</span>{/if}{clock}</span>
      </div>
      <div class="alarm-state">{stateLabel}</div>
      {#if snoozed}
        <div class="alarm-detail">Alert resumes after the countdown</div>
      {/if}
    </div>
    <div class="alarm-actions">
      <button class="btn done" onclick={() => app.run(() => api.done(item.id, occurrence))}>Done</button>
      <div class="snooze-action"><SnoozeMenu onsnooze={(m) => app.run(() => api.snooze(item.id, m, occurrence))} /></div>
      <button class="btn restart" disabled={!canStart} onclick={() => act("start")}>Restart</button>
    </div>
  {:else}
    <div class="dial">
      <svg viewBox="0 0 100 100" aria-hidden="true">
        <circle cx="50" cy="50" r={R} class="track" />
        <circle cx="50" cy="50" r={R} class="progress" stroke-dasharray={C}
          stroke-dashoffset={C * (1 - fraction)} transform="rotate(-90 50 50)" />
      </svg>
      <div class="dial-value" class:long={clock.length > 5} role="timer" aria-live="off" aria-label={`${display} remaining`}>
        {#if days}<span class="days">{days}</span>{/if}
        <span>{clock}</span>
      </div>
    </div>
    <div class="info">
      <div class="timer-heading">
        <button class="title" title="Edit {item.title}" onclick={() => onedit(item)}>
          {#if tag}<TagDot color={tag.color} />{/if}
          <span>{item.title}</span>
        </button>
        <div class="more"><ItemActions bind:this={menu} {item} {onedit} /></div>
      </div>
      <div class="state">{stateLabel}</div>
      <SourceLink notes={item.notes} />
      <div class="timer-actions">
        {#if item.timer.state === "running"}
          <button class="btn btn-primary" onclick={() => act("pause")}>Pause</button>
          <button class="btn reset" onclick={() => act("reset")}>Reset</button>
        {:else if item.timer.state === "paused"}
          <button class="btn btn-primary" onclick={() => act("resume")}>Resume</button>
          <button class="btn reset" onclick={() => act("reset")}>Reset</button>
        {:else}
          <button class="btn btn-primary" disabled={!canStart} onclick={() => act("start")}>Start</button>
        {/if}
      </div>
    </div>
  {/if}
</article>

<style>
  .timer {
    --dial-size: 8.625rem;
    transition: background-color var(--dur-slow) ease-out, border-color var(--dur-slow) ease-out, color var(--dur-slow) ease-out;
    display: grid;
    grid-template-columns: var(--dial-size) minmax(0, 1fr);
    align-items: center;
    gap: 1.5rem;
    padding: 0.55rem 1rem;
    min-width: 0;
    border-radius: var(--radius);
  }
  .timer.due {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 0;
    padding: 0.4rem 0.85rem 0.53rem;
    background: var(--ringing-bg);
    border-color: transparent;
    color: var(--ringing);
  }
  .timer.ringing {
    background: var(--alarm);
    color: var(--alarm-fg);
  }
  .alarm-header, .timer-heading {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.35rem;
  }
  .alarm-heading, .info { min-width: 0; }
  .title {
    display: flex;
    align-items: baseline;
    gap: 0.4rem;
    min-width: 0;
    min-height: var(--hit);
    padding: 0;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: inherit;
    font-family: var(--font-ui);
    font-size: 1.5rem;
    font-weight: 700;
    line-height: 1.15;
    text-align: left;
    cursor: pointer;
  }
  .title span {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    min-width: 0;
    overflow: hidden;
    overflow-wrap: anywhere;
  }
  @media (hover: hover) { .title:hover { text-decoration: underline; text-underline-offset: 0.16em; } }
  .more { flex: none; margin-top: -0.3rem; margin-right: -0.35rem; }
  .more :global(> .icon-btn) { width: var(--hit); height: var(--hit); }
  .alarm-header .title { font-size: 1.65rem; }
  .due .more :global(> .icon-btn) { color: inherit; }
  @media (hover: hover) { .ringing .more :global(> .icon-btn:hover) { background: color-mix(in srgb, var(--alarm-fg) 12%, transparent); } }
  .alarm-heading :global(.source-link) { color: inherit; }
  .dial {
    position: relative;
    display: grid;
    place-items: center;
    width: var(--dial-size);
    height: var(--dial-size);
  }
  .dial svg { position: absolute; inset: 0; width: 100%; height: 100%; }
  .track, .progress { fill: none; stroke-width: 7; }
  .track { stroke: var(--timer-track); }
  /* Starting, resetting and editing move the arc in one eased sweep… */
  .progress {
    stroke: var(--accent);
    stroke-linecap: round;
    transition: stroke-dashoffset 0.6s var(--ease-out), stroke var(--dur) ease-out;
  }
  /* …while a running timer drains continuously, one second per tick. */
  .running .progress { transition: stroke-dashoffset 1s linear, stroke var(--dur) ease-out; }
  .paused .progress { stroke: var(--fg-muted); }
  .dial-value, .alarm-time {
    display: flex;
    flex-direction: column;
    align-items: center;
    font-family: var(--font-timer);
    font-weight: 700;
    font-variant-numeric: lining-nums tabular-nums;
    letter-spacing: -0.025em;
    line-height: 1;
  }
  .dial-value { font-size: 2.5rem; }
  .dial-value.long { font-size: 1.55rem; }
  .days { margin-bottom: 0.2rem; font-size: 0.55em; letter-spacing: 0; }
  .state {
    margin-top: 0.2rem;
    color: var(--fg-muted);
    font-family: var(--font-ui);
    font-size: 1.15rem;
    font-weight: 700;
    line-height: 1.25;
  }
  .running .state { color: var(--accent); }
  .timer-actions { display: flex; gap: 0.6rem; margin-top: 0.9rem; }
  .timer-actions .btn { flex: 1 1 0; min-width: 0; }
  .timer-actions .btn-primary { flex-grow: 1.1; }
  .timer-actions .reset { background: var(--bg-sunken); }
  .timer-actions .btn, .alarm-actions .btn, .snooze-action :global(> .btn) {
    min-height: 2.75rem;
    padding: 0.45rem 0.6rem;
    border-radius: var(--radius-sm);
    font-family: var(--font-ui);
    font-size: 1.2rem;
    font-weight: 700;
    line-height: 1.2;
  }
  .alarm-display { display: grid; justify-items: center; padding: 0.15rem 0 0.8rem; }
  .alarm-time { font-size: clamp(5.5rem, 25vw, 7.375rem); line-height: 0.9; }
  .alarm-time.long { font-size: clamp(3rem, 13vw, 4.4rem); }
  /* The overdue sign is lighter than the digits so the time still leads. */
  .sign { font-weight: 600; opacity: 0.8; margin-right: 0.04em; }
  .alarm-state {
    margin-top: 0;
    font-family: var(--font-ui);
    font-size: 2rem;
    font-weight: 700;
    line-height: 1;
    text-align: center;
  }
  .alarm-detail { margin-top: 0.3rem; font-size: 0.85rem; text-align: center; }
  .alarm-actions { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 0.6rem; }
  .alarm-actions .btn { width: 100%; }
  .snooze-action { min-width: 0; }
  .snooze-action :global(> .btn) { width: 100%; }
  .snooze-action :global(> .btn > svg) { display: none; }
  .due .done { border-color: transparent; background: var(--ringing); color: var(--bg-raised); }
  .due .restart, .due .snooze-action :global(> .btn) {
    border-color: color-mix(in srgb, var(--ringing) 24%, transparent);
    background: transparent;
    color: inherit;
  }
  .ringing .done { background: var(--alarm-fg); color: #162b37; }
  .ringing .restart, .ringing .snooze-action :global(> .btn) {
    border-color: transparent;
    background: color-mix(in srgb, var(--alarm) 82%, var(--alarm-fg));
  }
  @media (hover: hover) { .ringing .alarm-actions .btn:hover, .ringing .snooze-action :global(> .btn:hover) { filter: brightness(0.94); } }
  .ringing .alarm-actions .btn:active, .ringing .snooze-action :global(> .btn:active) { filter: brightness(0.88); }
  .ringing .title:focus-visible, .ringing .alarm-actions .btn:focus-visible,
  .ringing .more :global(> .icon-btn:focus-visible),
  .ringing .snooze-action :global(> .btn:focus-visible),
  .ringing .alarm-heading :global(.source-link:focus-visible) { outline-color: var(--alarm-fg); }
  @media (max-width: 420px) {
    .timer { --dial-size: 7.75rem; gap: 1rem; padding: 0.75rem; }
    .title { font-size: 1.5rem; }
    .dial-value { font-size: 2rem; }
    .dial-value.long { font-size: 1.35rem; }
    .timer-actions { gap: 0.45rem; }
    .timer-actions .btn { font-size: 1.1rem; }
    .alarm-actions { gap: 0.45rem; }
  }
  @media (prefers-reduced-motion: reduce) {
    .progress { transition: none; }
  }
</style>
