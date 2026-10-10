<script lang="ts">
  import { untrack } from "svelte";
  import type { Countdown, CountdownDisplay, CountdownDraft, CountUnit } from "../lib/api/types";
  import { api, errorText } from "../lib/api/commands";
  import { emptyDraft, readCountdown, UNITS } from "../lib/countdown";
  import { toCivilTime, toDateInput, toTimeInput } from "../lib/dates";
  import { app } from "../lib/stores/app.svelte";
  import CountdownCard from "../lib/components/CountdownCard.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import Modal from "../lib/components/Modal.svelte";

  let { open = $bindable(false), countdown = null }: { open?: boolean; countdown?: Countdown | null } = $props();

  const COLORS = ["#e0483e", "#f08c2e", "#e8b930", "#3cb371", "#2fa4c9", "#3a86ff", "#8a63d2", "#d05ea0", "#8d8d93", "#162b37"];
  const ICONS = ["🎂", "🎉", "✈️", "🏖️", "🎄", "💍", "❤️", "🎓", "💼", "🏠", "🐶", "👶", "🏃", "💪", "🚭", "📅", "⏳", "🎮", "📚", "🩺", "💰", "🌱", "⭐", "🔥"];
  const UNIT_NAMES: Record<CountUnit, string> = { years: "Years", months: "Months", weeks: "Weeks", days: "Days", hours: "Hours", minutes: "Minutes", seconds: "Seconds" };
  const PRECISION: { label: string; largest: CountUnit; smallest: CountUnit }[] = [
    { label: "Days", largest: "days", smallest: "days" },
    { label: "Weeks + days", largest: "weeks", smallest: "days" },
    { label: "Years, months, days", largest: "years", smallest: "days" },
    { label: "Days to seconds", largest: "days", smallest: "seconds" },
    { label: "Everything", largest: "years", smallest: "seconds" },
  ];

  let draft = $state<CountdownDraft>(emptyDraft(toDateInput(Date.now())));
  let error = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    if (open) untrack(reset);
  });

  function reset() {
    error = null;
    if (countdown) {
      const { id: _id, deleted: _deleted, ...rest } = countdown;
      draft = structuredClone($state.snapshot(rest));
    } else {
      draft = emptyDraft(toDateInput(Date.now()));
    }
  }

  function setDisplay(patch: Partial<CountdownDisplay>) {
    draft.display = { ...draft.display, ...patch };
  }

  function setLargest(unit: CountUnit) {
    const smallest = UNITS.indexOf(draft.display.smallest) < UNITS.indexOf(unit) ? unit : draft.display.smallest;
    setDisplay({ largest: unit, smallest });
  }

  const validDate = $derived(/^\d{4}-\d{2}-\d{2}$/.test(draft.date));
  const preview = $derived(
    validDate ? { countdown: { ...draft, title: draft.title.trim() || "Countdown", id: "preview", deleted: false } as Countdown } : null,
  );
  const reading = $derived(preview ? readCountdown(preview.countdown, app.now) : null);
  const repeats = $derived(draft.repeat !== "none");
  const customColor = $derived(draft.display.color !== null && !COLORS.includes(draft.display.color));

  async function save() {
    if (busy) return;
    if (!draft.title.trim()) return (error = "Give it a name.");
    if (!validDate) return (error = "Pick a date.");
    busy = true;
    try {
      await api.saveCountdown(countdown?.id ?? null, $state.snapshot(draft));
      open = false;
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal bind:open title={countdown ? "Edit countdown" : "New countdown"}>
  {#if preview && reading}
    <div class="preview" inert><CountdownCard countdown={preview.countdown} {reading} /></div>
  {/if}

  <label class="field">
    <span>Name</span>
    <input class="input" bind:value={draft.title} placeholder="Mum's birthday, Japan trip, quit smoking…" onkeydown={(e) => e.key === "Enter" && save()} />
  </label>

  <div class="pair">
    <label class="field">
      <span>Date</span>
      <input class="input" type="date" bind:value={draft.date} />
    </label>
    <label class="field">
      <span>Repeats</span>
      <select class="input" bind:value={draft.repeat}>
        <option value="none">Doesn't repeat</option>
        <option value="weekly">Every week</option>
        <option value="monthly">Every month</option>
        <option value="yearly">Every year</option>
      </select>
    </label>
  </div>

  <label class="switch">
    <span>At a set time</span>
    <input type="checkbox" checked={draft.time !== null} onchange={(e) => (draft.time = e.currentTarget.checked ? "09:00:00" : null)} />
  </label>
  {#if draft.time !== null}
    <label class="field">
      <span>Time</span>
      <input class="input" type="time" value={toTimeInput(draft.time)} onchange={(e) => (draft.time = toCivilTime(e.currentTarget.value))} />
    </label>
  {/if}

  <div class="field">
    <span>Count</span>
    <div class="segmented" role="group" aria-label="Count direction">
      <button type="button" aria-pressed={draft.direction === "auto"} onclick={() => (draft.direction = "auto")}>Automatic</button>
      <button type="button" aria-pressed={draft.direction === "until"} onclick={() => (draft.direction = "until")}>Down to it</button>
      <button type="button" aria-pressed={draft.direction === "since"} onclick={() => (draft.direction = "since")}>Up from it</button>
    </div>
    <p class="hint muted">
      {#if draft.direction === "auto"}
        Counts down until the date, then up from it. {repeats ? "A repeat always counts down to the next one." : ""}
      {:else if draft.direction === "until"}
        {repeats ? "Counts down to the next one." : "Counts down; once the date has passed, shows how long ago it was."}
      {:else}
        {repeats ? "Counts up from the latest one." : "Counts up from the date; before it arrives, shows how long is left."}
      {/if}
    </p>
  </div>
  <label class="switch">
    <span>Count the date itself as day 1</span>
    <input type="checkbox" checked={draft.display.countFirstDay} onchange={(e) => setDisplay({ countFirstDay: e.currentTarget.checked })} />
  </label>

  <h3>Display</h3>

  <div class="field">
    <span>Size</span>
    <div class="segmented" role="group" aria-label="Size">
      <button type="button" aria-pressed={draft.display.size === "large"} onclick={() => setDisplay({ size: "large" })}>Large</button>
      <button type="button" aria-pressed={draft.display.size === "medium"} onclick={() => setDisplay({ size: "medium" })}>Card</button>
      <button type="button" aria-pressed={draft.display.size === "compact"} onclick={() => setDisplay({ size: "compact" })}>Compact</button>
    </div>
  </div>

  <div class="field">
    <span>Detail</span>
    <div class="chips" role="group" aria-label="Detail presets">
      {#each PRECISION as p (p.label)}
        <button
          type="button"
          class="chip"
          aria-pressed={draft.display.largest === p.largest && draft.display.smallest === p.smallest}
          onclick={() => setDisplay({ largest: p.largest, smallest: p.smallest })}>{p.label}</button>
      {/each}
    </div>
    <div class="pair">
      <label class="field">
        <span>From</span>
        <select class="input" value={draft.display.largest} onchange={(e) => setLargest(e.currentTarget.value as CountUnit)}>
          {#each UNITS as u (u)}<option value={u}>{UNIT_NAMES[u]}</option>{/each}
        </select>
      </label>
      <label class="field">
        <span>Down to</span>
        <select class="input" value={draft.display.smallest} onchange={(e) => setDisplay({ smallest: e.currentTarget.value as CountUnit })}>
          {#each UNITS.slice(UNITS.indexOf(draft.display.largest)) as u (u)}<option value={u}>{UNIT_NAMES[u]}</option>{/each}
        </select>
      </label>
    </div>
    <label class="switch">
      <span>Short unit labels (3y 2mo)</span>
      <input type="checkbox" checked={!draft.display.longUnits} onchange={(e) => setDisplay({ longUnits: !e.currentTarget.checked })} />
    </label>
  </div>

  <div class="field">
    <span>Icon</span>
    <div class="chips icons" role="group" aria-label="Icon">
      <button type="button" class="chip" aria-pressed={draft.display.icon === null} onclick={() => setDisplay({ icon: null })}>None</button>
      {#each ICONS as i (i)}
        <button type="button" class="chip emoji" aria-pressed={draft.display.icon === i} aria-label={i} onclick={() => setDisplay({ icon: i })}>{i}</button>
      {/each}
    </div>
    <input
      class="input"
      maxlength="16"
      placeholder="Or type any emoji or short text"
      value={draft.display.icon && !ICONS.includes(draft.display.icon) ? draft.display.icon : ""}
      oninput={(e) => setDisplay({ icon: e.currentTarget.value.trim() || null })}
    />
  </div>

  <div class="field">
    <span>Color</span>
    <div class="chips" role="group" aria-label="Color">
      <button type="button" class="chip" aria-pressed={draft.display.color === null} onclick={() => setDisplay({ color: null })}>Theme</button>
      {#each COLORS as c (c)}
        <button type="button" class="swatch" style:background={c} aria-pressed={draft.display.color === c} aria-label={c} onclick={() => setDisplay({ color: c })}></button>
      {/each}
      <label class="swatch custom" class:on={customColor} style:background={customColor ? draft.display.color : undefined} title="Any color">
        <span class="sr-only">Any color</span>
        <input type="color" value={draft.display.color ?? "#287d87"} oninput={(e) => setDisplay({ color: e.currentTarget.value })} />
      </label>
    </div>
  </div>

  <div class="field">
    <span>Fill</span>
    <div class="segmented" role="group" aria-label="Fill">
      <button type="button" aria-pressed={draft.display.fill === "solid"} onclick={() => setDisplay({ fill: "solid" })}>Solid</button>
      <button type="button" aria-pressed={draft.display.fill === "soft"} onclick={() => setDisplay({ fill: "soft" })}>Soft</button>
      <button type="button" aria-pressed={draft.display.fill === "outline"} onclick={() => setDisplay({ fill: "outline" })}>Outline</button>
    </div>
  </div>

  <label class="switch">
    <span>Show the date</span>
    <input type="checkbox" checked={draft.display.showDate} onchange={(e) => setDisplay({ showDate: e.currentTarget.checked })} />
  </label>
  {#if repeats}
    <label class="switch">
      <span>Number each one (66th)</span>
      <input type="checkbox" checked={draft.display.showOrdinal} onchange={(e) => setDisplay({ showOrdinal: e.currentTarget.checked })} />
    </label>
    <label class="switch">
      <span>Progress bar to the next one</span>
      <input type="checkbox" checked={draft.display.showProgress} onchange={(e) => setDisplay({ showProgress: e.currentTarget.checked })} />
    </label>
  {/if}
  <label class="switch">
    <span>Pin to the top</span>
    <input type="checkbox" bind:checked={draft.pinned} />
  </label>

  <label class="field">
    <span>Notes (shown on large)</span>
    <textarea class="input" rows="2" bind:value={draft.notes}></textarea>
  </label>

  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#snippet footer()}
    {#if countdown}
      <button
        class="btn btn-danger btn-quiet"
        onclick={async () => {
          const { title } = countdown!;
          await app.run(() => api.deleteCountdown(countdown!.id));
          open = false;
          app.notify(`Deleted “${title}”`);
        }}><Icon name="trash" size={16} /> Delete</button
      >
      <span class="spacer"></span>
    {/if}
    <button class="btn" onclick={() => (open = false)}>Cancel</button>
    <button class="btn btn-primary" onclick={save} disabled={busy}>{busy ? "Saving…" : "Save"}</button>
  {/snippet}
</Modal>

<style>
  /* Stays in view while scrolling, covering the body's top padding too. */
  .preview {
    position: sticky;
    top: -0.5rem;
    z-index: 1;
    margin-top: -0.5rem;
    padding: 0.5rem 0 0.35rem;
    background: var(--bg);
  }
  h3 {
    margin: 0.5rem 0 0;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-muted);
  }
  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.6rem;
  }
  .hint {
    margin: 0;
    font-size: 0.83rem;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    min-height: 36px;
    padding: 0.3rem 0.7rem;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--bg-raised);
    color: var(--fg);
    cursor: pointer;
  }
  .chip[aria-pressed="true"] {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-fg);
    font-weight: 700;
  }
  .chip.emoji {
    justify-content: center;
    min-width: 40px;
    padding: 0.2rem;
    font-size: 1.25rem;
  }
  .chip.emoji[aria-pressed="true"] {
    background: color-mix(in srgb, var(--accent) 22%, var(--bg-raised));
    color: var(--fg);
  }
  .swatch {
    position: relative;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-sm);
    border: 1px solid color-mix(in srgb, var(--fg) 20%, transparent);
    cursor: pointer;
  }
  .swatch[aria-pressed="true"], .swatch.on {
    outline: 3px solid var(--fg);
    outline-offset: 2px;
  }
  .swatch.custom:not(.on) {
    background: conic-gradient(#e0483e, #e8b930, #3cb371, #2fa4c9, #8a63d2, #d05ea0, #e0483e);
  }
  .swatch input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
  }
  .error {
    margin: 0;
    color: var(--ringing);
  }
  .spacer {
    flex: 1;
  }
</style>
