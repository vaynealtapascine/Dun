<script lang="ts">
  import type { Rule } from "../api/types";
  import { toCivilTime, toTimeInput } from "../dates";
  import { ruleSummary } from "../format";
  import Icon from "./Icon.svelte";

  let { rule = $bindable() }: { rule: Rule } = $props();

  type Kind = "minutes" | "hours" | "daily" | "weekly" | "monthly" | "yearly";
  const KINDS: [Kind, string][] = [
    ["daily", "Daily"],
    ["weekly", "Weekly"],
    ["monthly", "Monthly"],
    ["yearly", "Yearly"],
    ["hours", "Every few hours"],
    ["minutes", "Every few minutes"],
  ];
  const WEEKDAYS = ["M", "T", "W", "T", "F", "S", "S"];
  const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

  const kind = $derived<Kind>(rule.kind === "interval" ? rule.unit : rule.kind);
  const times = $derived(rule.kind === "interval" ? [] : rule.times);

  function setKind(k: Kind) {
    const keep = rule.kind === "interval" ? ["09:00:00"] : rule.times;
    const every = "every" in rule ? rule.every : 1;
    const today = new Date();
    rule =
      k === "minutes" || k === "hours"
        ? { kind: "interval", every: k === "minutes" ? 30 : 2, unit: k }
        : k === "daily"
          ? { kind: "daily", every, times: keep }
          : k === "weekly"
            ? { kind: "weekly", every, weekdays: [((today.getDay() + 6) % 7) + 1], times: keep }
            : k === "monthly"
              ? { kind: "monthly", every, day: today.getDate(), times: keep }
              : { kind: "yearly", month: today.getMonth() + 1, day: today.getDate(), times: keep };
  }

  function setTimes(next: string[]) {
    if (rule.kind !== "interval") rule = { ...rule, times: next };
  }

  function toggleDay(day: number) {
    if (rule.kind !== "weekly") return;
    const has = rule.weekdays.includes(day);
    const weekdays = has ? rule.weekdays.filter((d) => d !== day) : [...rule.weekdays, day].sort((a, b) => a - b);
    rule = { ...rule, weekdays };
  }
</script>

<div class="editor">
  <label class="field">
    <span>Repeats</span>
    <select class="input" value={kind} onchange={(e) => setKind(e.currentTarget.value as Kind)}>
      {#each KINDS as [k, label] (k)}
        <option value={k}>{label}</option>
      {/each}
    </select>
  </label>

  {#if rule.kind === "interval"}
    <label class="field">
      <span>Every</span>
      <div class="inline">
        <input
          class="input num"
          type="number"
          min="1"
          max={rule.unit === "minutes" ? 10000 : 1000}
          value={rule.every}
          oninput={(e) => rule.kind === "interval" && (rule = { ...rule, every: Number(e.currentTarget.value) || 1 })}
        />
        <span class="muted">{rule.unit}</span>
      </div>
    </label>
  {:else if rule.kind !== "yearly"}
    <label class="field">
      <span>Every</span>
      <div class="inline">
        <input
          class="input num"
          type="number"
          min="1"
          value={rule.every}
          oninput={(e) => rule.kind !== "interval" && rule.kind !== "yearly" && (rule = { ...rule, every: Number(e.currentTarget.value) || 1 })}
        />
        <span class="muted">{rule.kind === "daily" ? "days" : rule.kind === "weekly" ? "weeks" : "months"}</span>
      </div>
    </label>
  {/if}

  {#if rule.kind === "weekly"}
    <div class="field">
      <span>On</span>
      <div class="days">
        {#each WEEKDAYS as label, i (i)}
          <button
            type="button"
            class="day"
            aria-pressed={rule.weekdays.includes(i + 1)}
            aria-label={["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"][i]}
            onclick={() => toggleDay(i + 1)}>{label}</button
          >
        {/each}
        <button type="button" class="btn btn-quiet small" onclick={() => rule.kind === "weekly" && (rule = { ...rule, weekdays: [1, 2, 3, 4, 5] })}>Weekdays</button>
      </div>
    </div>
  {:else if rule.kind === "monthly"}
    <label class="field">
      <span>On day</span>
      <input
        class="input num"
        type="number"
        min="1"
        max="31"
        value={rule.day}
        oninput={(e) => rule.kind === "monthly" && (rule = { ...rule, day: Math.min(31, Math.max(1, Number(e.currentTarget.value) || 1)) })}
      />
    </label>
  {:else if rule.kind === "yearly"}
    <div class="field">
      <span>On</span>
      <div class="inline">
        <select
          class="input"
          value={rule.month}
          onchange={(e) => rule.kind === "yearly" && (rule = { ...rule, month: Number(e.currentTarget.value) })}
        >
          {#each MONTHS as m, i (m)}
            <option value={i + 1}>{m}</option>
          {/each}
        </select>
        <input
          class="input num"
          type="number"
          min="1"
          max="31"
          value={rule.day}
          oninput={(e) => rule.kind === "yearly" && (rule = { ...rule, day: Number(e.currentTarget.value) || 1 })}
        />
      </div>
    </div>
  {/if}

  {#if rule.kind !== "interval"}
    <div class="field">
      <span>At</span>
      <div class="times">
        {#each times as t, i (i)}
          <div class="time">
            <input
              class="input"
              type="time"
              value={toTimeInput(t)}
              onchange={(e) => setTimes(times.map((x, j) => (j === i ? toCivilTime(e.currentTarget.value) : x)))}
            />
            {#if times.length > 1}
              <button type="button" class="icon-btn" aria-label="Remove time" onclick={() => setTimes(times.filter((_, j) => j !== i))}>
                <Icon name="close" size={16} />
              </button>
            {/if}
          </div>
        {/each}
        {#if times.length < 24}
          <button type="button" class="btn btn-quiet small" onclick={() => setTimes([...times, times.at(-1) ?? "09:00:00"])}>
            <Icon name="plus" size={14} /> Add time
          </button>
        {/if}
      </div>
    </div>
  {/if}

  <p class="summary muted">{ruleSummary(rule)}</p>
</div>

<style>
  .editor {
    display: grid;
    gap: 0.7rem;
    padding: 0.8rem;
    border-radius: var(--radius);
    background: var(--bg-sunken);
  }
  .inline {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .num {
    width: 6rem;
  }
  .days {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
    align-items: center;
  }
  .day {
    width: 2.2rem;
    height: 2.2rem;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--bg-raised);
    cursor: pointer;
  }
  .day[aria-pressed="true"] {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-fg);
    font-weight: 600;
  }
  .times {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    align-items: center;
  }
  .time {
    display: flex;
    align-items: center;
  }
  .time .input {
    width: 8rem;
  }
  .small {
    min-height: 2rem;
    font-size: 0.85rem;
  }
  .summary {
    margin: 0;
    font-size: 0.85rem;
  }
</style>
