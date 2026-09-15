<script lang="ts">
  import { untrack } from "svelte";
  import type { ChimeOption, ChimeRef, ItemDraft, ItemView, Nag, RepeatMode, Rule, Schedule } from "../lib/api/types";
  import { api } from "../lib/api/commands";
  import { fromLocalInput, joinDuration, roundUpMinutes, splitDuration, toDateInput, toLocalInput } from "../lib/dates";
  import { relative } from "../lib/format";
  import { app } from "../lib/stores/app.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import Modal from "../lib/components/Modal.svelte";
  import RepeatEditor from "../lib/components/RepeatEditor.svelte";

  type Kind = "once" | "repeat" | "timer";

  let {
    open = $bindable(false),
    item = null,
    initialKind = "once",
    prefill = null,
  }: {
    open?: boolean;
    item?: ItemView | null;
    initialKind?: Kind;
    /** Values for a new item, e.g. what quick-add understood. Ignored when editing. */
    prefill?: Partial<ItemDraft> | null;
  } = $props();

  const NAG_INTERVALS = [1, 2, 3, 5, 10, 15, 30, 60];

  let kind = $state<Kind>("once");
  let title = $state("");
  let notes = $state("");
  let tag = $state<string | null>(null);
  let due = $state("");
  let rule = $state<Rule>({ kind: "daily", every: 1, times: ["09:00:00"] });
  let startDate = $state("");
  let startAt = $state("");
  let mode = $state<RepeatMode>("fromSchedule");
  let durH = $state(0);
  let durM = $state(10);
  let durS = $state(0);
  let startTimer = $state(true);
  let nagOn = $state(true);
  let nagInterval = $state(1);
  let chime = $state<ChimeRef | null>(null);
  let quietExempt = $state<boolean | null>(null);
  let chimes = $state<ChimeOption[]>([]);
  let busy = $state(false);
  let error = $state<string | null>(null);

  // Reset the form whenever it opens (only `open` is tracked).
  $effect(() => {
    if (open) untrack(reset);
  });

  function reset() {
    error = null;
    api.chimes().then((c) => (chimes = c)).catch(() => {});
    const now = app.now;
    const pre = item ? null : prefill;
    const nag: Nag = item?.nag ?? pre?.nag ?? app.snapshot?.settings.nagDefault ?? { mode: "repeat", intervalMin: 1 };
    nagOn = nag.mode === "repeat";
    nagInterval = nag.mode === "repeat" ? nag.intervalMin : 1;
    title = item?.title ?? pre?.title ?? "";
    notes = item?.notes ?? pre?.notes ?? "";
    tag = item?.tag ?? pre?.tag ?? null;
    chime = item?.chime ?? pre?.chime ?? null;
    quietExempt = item?.quietExemptOverride ?? pre?.quietExempt ?? null;
    due = toLocalInput(roundUpMinutes(now, 15));
    startDate = toDateInput(now);
    startAt = toLocalInput(roundUpMinutes(now, 15));
    mode = "fromSchedule";
    rule = { kind: "daily", every: 1, times: ["09:00:00"] };
    startTimer = pre?.startTimer ?? true;
    ({ h: durH, m: durM, s: durS } = splitDuration(10 * 60_000));

    const s = item?.schedule ?? pre?.schedule;
    kind = s ? (s.kind === "oneOff" ? "once" : s.kind === "recurring" ? "repeat" : "timer") : initialKind;
    if (s?.kind === "oneOff") due = toLocalInput(s.due);
    if (s?.kind === "recurring") {
      rule = s.recurrence.rule;
      mode = s.mode;
      startDate = s.recurrence.start.slice(0, 10);
      startAt = s.recurrence.start.slice(0, 16);
    }
    if (s?.kind === "timer") ({ h: durH, m: durM, s: durS } = splitDuration(s.durationMs));
  }

  const exemptDefault = $derived(kind === "timer");
  const dueMs = $derived(fromLocalInput(due));

  function buildSchedule(): Schedule | string {
    if (kind === "once") {
      if (dueMs == null) return "Pick a date and time.";
      return { kind: "oneOff", due: dueMs };
    }
    if (kind === "timer") {
      const durationMs = joinDuration(durH, durM, durS);
      if (durationMs < 1000) return "Set how long the timer runs.";
      return { kind: "timer", durationMs };
    }
    const start = rule.kind === "interval" ? `${startAt.slice(0, 16)}:00` : `${startDate}T00:00:00`;
    if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}$/.test(start)) return "Pick when the repeat starts.";
    return { kind: "recurring", recurrence: { rule, start, tz: null }, mode, effectiveFrom: 0 };
  }

  async function save() {
    const schedule = buildSchedule();
    if (typeof schedule === "string") {
      error = schedule;
      return;
    }
    if (!title.trim()) {
      error = "Give it a title.";
      return;
    }
    const draft: ItemDraft = {
      title: title.trim(),
      notes,
      tag,
      schedule,
      nag: nagOn ? { mode: "repeat", intervalMin: nagInterval } : { mode: "once" },
      chime,
      quietExempt,
      startTimer: kind === "timer" && !item && startTimer,
    };
    busy = true;
    try {
      if (item) await api.updateItem(item.id, draft);
      else await api.createItem(draft);
      open = false;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (!item) return;
    await app.run(() => api.deleteItem(item!.id));
    open = false;
  }

  const chimeKey = (c: ChimeRef | null) => (c == null ? "" : c.kind === "bundled" ? `b:${c.id}` : `c:${c.sha256}`);
</script>

<Modal bind:open title={item ? "Edit" : "New"}>
  {#if !item}
    <div class="segmented" role="group" aria-label="Kind">
      <button type="button" aria-pressed={kind === "once"} onclick={() => (kind = "once")}>Reminder</button>
      <button type="button" aria-pressed={kind === "repeat"} onclick={() => (kind = "repeat")}>Repeating</button>
      <button type="button" aria-pressed={kind === "timer"} onclick={() => (kind = "timer")}>Timer</button>
    </div>
  {/if}

  <label class="field">
    <span>Title</span>
    <!-- svelte-ignore a11y_autofocus -->
    <input
      class="input"
      bind:value={title}
      placeholder={kind === "timer" ? "Laundry" : "Pay rent"}
      autofocus
      onkeydown={(e) => e.key === "Enter" && save()}
    />
  </label>

  {#if kind === "once"}
    <label class="field">
      <span>When {#if dueMs}<em class="muted">({relative(dueMs, app.now)})</em>{/if}</span>
      <input class="input" type="datetime-local" bind:value={due} />
    </label>
  {:else if kind === "repeat"}
    <RepeatEditor bind:rule />
    {#if rule.kind === "interval"}
      <label class="field">
        <span>Starting</span>
        <input class="input" type="datetime-local" bind:value={startAt} />
      </label>
    {:else}
      <label class="field">
        <span>Starting</span>
        <input class="input" type="date" bind:value={startDate} />
      </label>
    {/if}
    <div class="segmented" role="group" aria-label="Repeat counting">
      <button type="button" aria-pressed={mode === "fromSchedule"} onclick={() => (mode = "fromSchedule")}>On schedule</button>
      <button type="button" aria-pressed={mode === "afterCompletion"} onclick={() => (mode = "afterCompletion")}>After I finish</button>
    </div>
  {:else}
    <div class="field">
      <span>Duration</span>
      <div class="duration">
        <label><input class="input" type="number" min="0" max="999" bind:value={durH} /> h</label>
        <label><input class="input" type="number" min="0" max="59" bind:value={durM} /> m</label>
        <label><input class="input" type="number" min="0" max="59" bind:value={durS} /> s</label>
      </div>
    </div>
    {#if !item}
      <label class="switch"><span>Start now</span><input type="checkbox" bind:checked={startTimer} /></label>
    {/if}
  {/if}

  <div class="group">
    <label class="switch">
      <span>Keep nagging until done</span>
      <input type="checkbox" bind:checked={nagOn} />
    </label>
    {#if nagOn}
      <label class="field">
        <span>Every</span>
        <select class="input" bind:value={nagInterval}>
          {#each NAG_INTERVALS as m (m)}
            <option value={m}>{m === 1 ? "minute" : `${m} minutes`}</option>
          {/each}
        </select>
      </label>
    {/if}
    <label class="switch">
      <span>Ring during quiet hours</span>
      <input
        type="checkbox"
        checked={quietExempt ?? exemptDefault}
        onchange={(e) => (quietExempt = e.currentTarget.checked === exemptDefault ? null : e.currentTarget.checked)}
      />
    </label>
    <div class="field">
      <span>Sound</span>
      <div class="inline">
        <select
          class="input"
          value={chimeKey(chime)}
          onchange={(e) => (chime = chimes.find((c) => chimeKey(c.chime) === e.currentTarget.value)?.chime ?? null)}
        >
          <option value="">Default</option>
          {#each chimes as c (chimeKey(c.chime))}
            <option value={chimeKey(c.chime)}>{c.label}</option>
          {/each}
        </select>
        <button type="button" class="icon-btn" aria-label="Play sound" onclick={() => api.playChime(chime)}>
          <Icon name="play" size={16} />
        </button>
      </div>
    </div>
  </div>

  {#if (app.snapshot?.tags.length ?? 0) > 0}
    <label class="field">
      <span>Tag</span>
      <select class="input" bind:value={tag}>
        <option value={null}>None</option>
        {#each app.snapshot?.tags ?? [] as t (t.id)}
          <option value={t.id}>{t.name}</option>
        {/each}
      </select>
    </label>
  {/if}

  <label class="field">
    <span>Notes</span>
    <textarea class="input" bind:value={notes} rows="2"></textarea>
  </label>

  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#snippet footer()}
    {#if item}
      <button class="btn btn-danger btn-quiet" onclick={remove}><Icon name="trash" size={16} /> Delete</button>
      <span class="spacer"></span>
    {/if}
    <button class="btn" onclick={() => (open = false)}>Cancel</button>
    <button class="btn btn-primary" onclick={save} disabled={busy}>{item ? "Save" : "Add"}</button>
  {/snippet}
</Modal>

<style>
  .group {
    display: grid;
    gap: 0.4rem;
    padding: 0.4rem 0.8rem 0.7rem;
    border-radius: var(--radius);
    background: var(--bg-raised);
    border: 1px solid var(--border);
  }
  .duration {
    display: flex;
    gap: 0.5rem;
  }
  .duration label {
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }
  .duration .input {
    width: 4.5rem;
  }
  .inline {
    display: flex;
    gap: 0.4rem;
    align-items: center;
  }
  .error {
    margin: 0;
    color: var(--ringing);
  }
  .spacer {
    flex: 1;
  }
  em {
    font-style: normal;
  }
</style>
