<script lang="ts">
  import type { Countdown } from "../api/types";
  import { type CountReading, readingText, unitLabel, visibleParts } from "../countdown";
  import { ordinal } from "../format";
  import Icon from "./Icon.svelte";

  let { countdown, reading, onedit }: { countdown: Countdown; reading: CountReading; onedit?: (c: Countdown) => void } = $props();

  const d = $derived(countdown.display);
  const parts = $derived(visibleParts(reading.parts));
  /** On the day itself, unless it's being counted as day 1. */
  const today = $derived(reading.isNow && !(reading.mode === "since" && d.countFirstDay));
  const showOrdinal = $derived(d.showOrdinal && countdown.repeat !== "none" && reading.index > 0);
  const progress = $derived(d.showProgress ? reading.progress : null);
  const totalText = $derived(reading.total?.started ? readingText(reading.total.parts, d.longUnits) : "Not started yet");
  const totalCaption = $derived(reading.total && d.showDate
    ? `Total since ${reading.total.start.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })}${countdown.time ? `, ${reading.total.start.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}` : ""}`
    : "Total duration");

  /** Dark or light ink, whichever reads better on the chosen banner color. */
  function inkOn(hex: string): string {
    const full = hex.length === 4 ? hex.replace(/^#(.)(.)(.)$/, "#$1$1$2$2$3$3") : hex;
    const [r, g, b] = [1, 3, 5].map((i) => {
      const c = parseInt(full.slice(i, i + 2), 16) / 255;
      return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    }) as [number, number, number];
    return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.4 ? "#10202a" : "#ffffff";
  }

  const dateText = $derived.by(() => {
    const t = reading.target;
    const sameYear = t.getFullYear() === new Date().getFullYear();
    const date = t.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric", year: sameYear ? undefined : "numeric" });
    return countdown.time ? `${date}, ${t.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}` : date;
  });

  const caption = $derived.by(() => {
    if (today) return d.showDate ? `Today · ${dateText}` : "Today";
    if (reading.mode === "until") return d.showDate ? `until ${dateText}` : "to go";
    if (reading.flipped || countdown.direction === "until") return d.showDate ? `ago · ${dateText}` : "ago";
    return d.showDate ? `since ${dateText}` : "so far";
  });

  const spoken = $derived(`${countdown.title}: ${today ? "today" : `${readingText(reading.parts, true)} ${caption}`}${reading.total ? `. ${totalCaption}: ${reading.total.started ? readingText(reading.total.parts, true) : "Not started yet"}` : ""}`);
</script>

<button
  class="countdown {d.size} {d.fill}"
  class:wide={!today && parts.length > 3}
  style:--cd={d.color ?? undefined}
  style:--cd-ink={d.color ? inkOn(d.color) : undefined}
  aria-label={spoken}
  onclick={() => onedit?.(countdown)}
>
  {#if d.icon}<span class="icon" aria-hidden="true">{d.icon}</span>{/if}
  <span class="head">
    <span class="title">
      {#if countdown.pinned}<span class="pin"><Icon name="pin" size={14} /></span>{/if}
      <span class="name">{countdown.title}</span>
      {#if showOrdinal}<span class="ordinal">{ordinal(reading.index)}</span>{/if}
    </span>
    {#if d.size !== "compact"}
      <span class="caption">{caption}</span>
      {#if countdown.notes && d.size === "large"}<span class="notes">{countdown.notes}</span>{/if}
    {/if}
  </span>
  <span class="reading" aria-hidden="true">
    {#if today}
      <span class="part"><span class="num">Today</span></span>
    {:else}
      {#each parts as p (p.unit)}
        <span class="part"><span class="num">{p.value}</span><span class="unit" class:short={!d.longUnits}>{unitLabel(p.unit, p.value, d.longUnits)}</span></span>
      {/each}
      {#if d.size === "compact"}<span class="unit tail">{reading.mode === "until" ? "left" : "ago"}</span>{/if}
    {/if}
  </span>
  {#if reading.total}
    <span class="total" aria-hidden="true">
      <span class="total-caption">{totalCaption}</span>
      <span class="total-reading">{totalText}</span>
    </span>
  {/if}
  {#if progress !== null}
    <span class="progress" aria-hidden="true"><span style:width="{progress * 100}%"></span></span>
  {/if}
</button>

<style>
  .countdown {
    --cd: var(--accent);
    --cd-ink: var(--accent-fg);
    position: relative;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 0.25rem 0.75rem;
    width: 100%;
    min-width: 0;
    overflow: hidden;
    border: 1px solid transparent;
    border-radius: var(--radius);
    color: var(--fg);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition: transform var(--dur-fast) var(--ease-out), filter var(--dur-fast) ease-out;
  }
  .countdown:active { transform: scale(0.985); }
  @media (hover: hover) { .countdown:hover { filter: brightness(0.97); } }
  .countdown:not(:has(.icon)) { grid-template-columns: 1fr auto; }

  /* Fills */
  .solid { background: var(--cd); color: var(--cd-ink); }
  .soft { background: color-mix(in srgb, var(--cd) 15%, var(--bg-raised)); border-color: color-mix(in srgb, var(--cd) 35%, var(--border)); }
  .outline { background: var(--bg-raised); border: 2px solid var(--cd); }
  .soft .num, .outline .num, .soft .ordinal, .outline .ordinal { color: color-mix(in srgb, var(--cd) 78%, var(--fg)); }

  .head { display: grid; min-width: 0; gap: 0.1rem; }
  .title { display: flex; align-items: center; gap: 0.35rem; min-width: 0; font-weight: 700; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pin { display: inline-flex; flex: none; opacity: 0.75; }
  .ordinal { flex: none; padding: 0 0.35rem; border-radius: 4px; border: 1px solid currentColor; font-size: 0.75em; font-weight: 700; opacity: 0.85; }
  .caption, .notes { font-size: 0.85rem; opacity: 0.8; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .notes { white-space: normal; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .icon { display: grid; place-items: center; line-height: 1; }

  .reading { display: flex; flex-wrap: wrap; justify-content: flex-end; align-items: baseline; min-width: 0; gap: 0 0.5rem; font-variant-numeric: lining-nums tabular-nums; }
  .part { display: inline-flex; align-items: baseline; gap: 0.2rem; white-space: nowrap; }
  .num { font-weight: 700; letter-spacing: -0.025em; line-height: 1; }
  .unit { font-size: 0.85rem; opacity: 0.8; }
  .unit.short { margin-left: -0.15rem; }
  .total { grid-column: 1 / -1; display: flex; flex-wrap: wrap; align-items: baseline; min-width: 0; gap: 0.15rem 0.6rem; padding-top: 0.35rem; border-top: 1px solid color-mix(in srgb, currentColor 25%, transparent); }
  .total-caption { font-size: 0.8rem; }
  .total-reading { min-width: 0; font-size: 0.95rem; font-weight: 600; font-variant-numeric: lining-nums tabular-nums; }
  .compact .total { padding-top: 0.25rem; }
  .compact:has(.total) { padding-bottom: 0.65rem; }

  /* Sizes */
  .large { grid-template-columns: auto 1fr; padding: 1rem 1.1rem 1.1rem; gap: 0.5rem 0.9rem; }
  .large:not(:has(.icon)) { grid-template-columns: 1fr; }
  .large .icon { font-size: 2.4rem; }
  .large .title { font-size: 1.25rem; }
  .large .reading { grid-column: 1 / -1; justify-content: flex-start; gap: 0 0.9rem; }
  .large .num { font-size: clamp(2.8rem, 12vw, 4rem); }
  .large .unit { font-size: 1rem; }

  .medium { padding: 0.7rem 0.9rem; }
  .medium .icon { font-size: 1.75rem; }
  .medium .num { font-size: 2rem; }

  /* Many units need the full width; the numbers move under the title. */
  .medium.wide { grid-template-columns: auto 1fr; }
  .medium.wide:not(:has(.icon)) { grid-template-columns: 1fr; }
  .medium.wide .reading { grid-column: 1 / -1; justify-content: flex-start; }
  .medium.wide .num { font-size: 1.75rem; }

  .compact { padding: 0.45rem 0.8rem; min-height: var(--hit); }
  .compact.wide { grid-template-columns: auto minmax(0, 1fr); }
  .compact.wide:not(:has(.icon)) { grid-template-columns: minmax(0, 1fr); }
  .compact.wide .reading { grid-column: 1 / -1; justify-content: flex-start; }
  .compact .icon { font-size: 1.2rem; }
  .compact .num { font-size: 1.1rem; }
  .compact .tail { margin-left: -0.2rem; }

  .progress { grid-column: 1 / -1; display: block; height: 4px; margin-top: 0.3rem; border-radius: 2px; background: color-mix(in srgb, currentColor 18%, transparent); overflow: hidden; }
  .progress span { display: block; height: 100%; background: currentColor; opacity: 0.7; }
  .soft .progress span, .outline .progress span { background: var(--cd); opacity: 1; }
  .compact .progress { position: absolute; left: 0; right: 0; bottom: 0; margin: 0; height: 3px; border-radius: 0; }

  @media (max-width: 380px) {
    .medium { grid-template-columns: auto 1fr; }
    .medium:not(:has(.icon)) { grid-template-columns: 1fr; }
    .medium .reading { grid-column: 1 / -1; justify-content: flex-start; }
  }
</style>
