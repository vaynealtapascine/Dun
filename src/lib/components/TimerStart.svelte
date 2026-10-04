<script lang="ts">
  import type { Snippet } from "svelte";
  import { api, errorText } from "../api/commands";
  import { timerName } from "../duration";
  import { app } from "../stores/app.svelte";
  import Icon from "./Icon.svelte";
  import { stepField } from "../gestures";

  let { children, onmanagepresets, managingPresets = false }: { children?: Snippet; onmanagepresets?: () => void; managingPresets?: boolean } = $props();

  let hours = $state("00");
  let minutes = $state("05");
  let seconds = $state("00");
  let busy = $state(false);
  let error = $state<string | null>(null);

  function durationFromFields(values: [string, string, string]): number | null {
    if (values.some((value) => !/^\d*$/.test(value.trim()))) return null;
    const h = Number(values[0].trim() || "0");
    const m = Number(values[1].trim() || "0");
    const s = Number(values[2].trim() || "0");
    if ([h, m, s].some((value) => !Number.isFinite(value) || !Number.isInteger(value) || value < 0)) return null;
    if (h > 999 || m > 59 || s > 59) return null;
    return (h * 3600 + m * 60 + s) * 1000;
  }

  function padField(field: "hours" | "minutes" | "seconds") {
    const value = { hours, minutes, seconds }[field];
    if (!/^\d*$/.test(value.trim())) return;
    const padded = (value.trim() || "0").padStart(2, "0");
    if (field === "hours") hours = padded;
    if (field === "minutes") minutes = padded;
    if (field === "seconds") seconds = padded;
  }

  const MAX = { hours: 999, minutes: 59, seconds: 59 } as const;
  type Field = keyof typeof MAX;

  function stepBy(field: Field, delta: number) {
    const next = stepField({ hours, minutes, seconds }[field], delta, MAX[field]);
    if (field === "hours") hours = next;
    if (field === "minutes") minutes = next;
    if (field === "seconds") seconds = next;
  }

  /** Up and Down nudge a field; with Shift, by ten. */
  function onkeydown(e: KeyboardEvent, field: Field) {
    if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
    e.preventDefault();
    stepBy(field, (e.key === "ArrowUp" ? 1 : -1) * (e.shiftKey ? 10 : 1));
  }

  /** The wheel turns a field only once it has focus, so scrolling past never changes it. */
  function wheelSteps(node: HTMLInputElement, field: Field) {
    function onwheel(e: WheelEvent) {
      if (document.activeElement !== node || e.deltaY === 0) return;
      e.preventDefault();
      stepBy(field, e.deltaY < 0 ? 1 : -1);
    }
    node.addEventListener("wheel", onwheel, { passive: false });
    return { destroy: () => node.removeEventListener("wheel", onwheel) };
  }

  /** Focusing a field selects it, so typing replaces "05" rather than appending to it. */
  const selectAll = (e: FocusEvent) => (e.currentTarget as HTMLInputElement).select();

  async function start() {
    if (busy) return;
    error = null;
    const durationMs = durationFromFields([hours, minutes, seconds]);
    if (durationMs === null) {
      error = "Use whole numbers: 0–999 hours and 0–59 minutes or seconds.";
      return;
    }
    if (durationMs < 1000) {
      error = "Set at least 1 second.";
      return;
    }
    busy = true;
    const title = timerName(durationMs);
    try {
      await api.createItem({
        title,
        notes: "",
        tag: null,
        schedule: { kind: "timer", durationMs },
        nag: null,
        chime: null,
        quietExempt: null,
        startTimer: true,
      });
      app.notify(`Started “${title}”`);
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }
</script>

<section class="timer-start card" aria-labelledby="new-timer-heading">
  <div class="start-heading">
    <h2 id="new-timer-heading">New timer</h2>
    {#if onmanagepresets}<button class="icon-btn" type="button" aria-label="Manage presets" aria-expanded={managingPresets} onclick={onmanagepresets}><Icon name="more" /></button>{/if}
  </div>
  <form onsubmit={(event) => { event.preventDefault(); void start(); }} novalidate aria-busy={busy}>
    <div class="duration-setter">
      <label>
        <input type="text" inputmode="numeric" autocomplete="off" maxlength="3" bind:value={hours} use:wheelSteps={"hours"}
          onfocus={selectAll} onkeydown={(e) => onkeydown(e, "hours")} onblur={() => padField("hours")} aria-describedby={error ? "timer-start-error" : undefined} disabled={busy} />
        <span>Hours</span>
      </label>
      <span class="separator" aria-hidden="true">:</span>
      <label>
        <input type="text" inputmode="numeric" autocomplete="off" maxlength="2" bind:value={minutes} use:wheelSteps={"minutes"}
          onfocus={selectAll} onkeydown={(e) => onkeydown(e, "minutes")} onblur={() => padField("minutes")} aria-describedby={error ? "timer-start-error" : undefined} disabled={busy} />
        <span>Minutes</span>
      </label>
      <span class="separator" aria-hidden="true">:</span>
      <label>
        <input type="text" inputmode="numeric" autocomplete="off" maxlength="2" bind:value={seconds} use:wheelSteps={"seconds"}
          onfocus={selectAll} onkeydown={(e) => onkeydown(e, "seconds")} onblur={() => padField("seconds")} aria-describedby={error ? "timer-start-error" : undefined} disabled={busy} />
        <span>Seconds</span>
      </label>
    </div>
    <button class="btn btn-primary start-button" type="submit" disabled={busy}>{busy ? "Starting…" : "Start timer"}</button>
    {#if error}<p class="start-error" id="timer-start-error" role="alert">{error}</p>{/if}
  </form>
  {#if children}<div class="preset-slot">{@render children()}</div>{/if}
</section>

<style>
  .timer-start {
    padding: 0.4rem 0.85rem;
  }
  .start-heading { display: flex; align-items: center; justify-content: space-between; margin-bottom: 0.35rem; }
  .start-heading .icon-btn { width: var(--hit); height: var(--hit); }
  h2 {
    margin: 0;
    font-family: var(--font-ui);
    font-size: 1.3rem;
    font-weight: 700;
    line-height: 1.1;
  }
  form {
    display: grid;
    grid-template-columns: minmax(0, 1.83fr) minmax(7rem, 1fr);
    column-gap: 0.8rem;
    align-items: start;
  }
  .duration-setter {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 0.4rem minmax(0, 1fr) 0.4rem minmax(0, 1fr);
    gap: 0.3rem;
    min-width: 0;
  }
  label {
    display: grid;
    gap: 0.1rem;
    min-width: 0;
    text-align: center;
  }
  label input {
    width: 100%;
    min-width: 0;
    height: 2.85rem;
    padding: 0.1rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-raised);
    color: var(--fg);
    text-align: center;
    font-family: var(--font-timer);
    font-weight: 700;
    font-size: 2.1rem;
    font-variant-numeric: lining-nums tabular-nums;
    line-height: 1;
    caret-color: var(--accent);
    transition: border-color var(--dur-fast) ease-out, background-color var(--dur-fast) ease-out;
  }
  label input:focus { border-color: var(--focus); }
  label input:disabled {
    opacity: 0.65;
  }
  label span {
    color: var(--fg-muted);
    font-size: 0.7rem;
    letter-spacing: -0.03em;
  }
  .separator {
    padding-top: 0.3rem;
    color: var(--fg-muted);
    font-family: var(--font-timer);
    font-weight: 700;
    font-size: 1.9rem;
    line-height: 2.7rem;
    text-align: center;
  }
  .start-button {
    width: 100%;
    min-height: 3.28rem;
    padding-inline: 0.45rem;
    font-size: 1.2rem;
    font-weight: 700;
  }
  .start-error {
    grid-column: 1 / -1;
    margin: 0.6rem 0 0;
    color: var(--ringing);
    font-size: 0.85rem;
  }
  .preset-slot {
    margin-top: 0.5rem;
  }
  @media (max-width: 399px) {
    .timer-start {
      padding-inline: 0.65rem;
    }
    form {
      column-gap: 0.55rem;
      grid-template-columns: minmax(0, 1.7fr) minmax(6.4rem, 1fr);
    }
    .duration-setter {
      grid-template-columns: minmax(0, 1fr) 0.25rem minmax(0, 1fr) 0.25rem minmax(0, 1fr);
      gap: 0.1rem;
    }
    label input {
      font-size: 1.7rem;
    }
    label span {
      font-size: 0.7rem;
    }
    .start-button {
      font-size: 0.9rem;
    }
  }
</style>
