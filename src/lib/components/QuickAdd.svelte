<script lang="ts">
  import type { ItemDraft, Ms } from "../api/types";
  import { api, errorText } from "../api/commands";
  import { formatShort } from "../duration";
  import { nagSummary, ruleSummary, when } from "../format";
  import { parseQuickAdd, timerAsReminder, type Parsed } from "../quickadd/parse";
  import { app } from "../stores/app.svelte";
  import Icon from "./Icon.svelte";

  let {
    onmore,
    onadded,
    onescape,
    autofocus = false,
    placeholder = "Add… e.g. Laundry in 45m, Stretch every weekday at 3pm",
  }: {
    /** Open the full form with what was typed so far. */
    onmore: (draft: Partial<ItemDraft>) => void;
    onadded?: (title: string) => void;
    onescape?: () => void;
    autofocus?: boolean;
    placeholder?: string;
  } = $props();

  const TAG_COLORS = ["#3cb371", "#3a86ff", "#f08c2e", "#8a63d2", "#e0483e", "#2fa4c9"];

  let text = $state("");
  let asReminder = $state(false);
  let preview = $state<Ms[] | null>(null);
  let previewError = $state<string | null>(null);
  let busy = $state(false);
  let input: HTMLInputElement | undefined = $state();

  export function focus() {
    input?.focus();
    input?.select();
  }

  export function clear() {
    text = "";
    asReminder = false;
  }

  function parseAt(now: number): Parsed | null {
    if (!text.trim()) return null;
    const p = parseQuickAdd(text, {
      now,
      tags: app.snapshot?.tags ?? [],
      dateOnlyTime: app.snapshot?.settings.dateOnlyTime ?? "09:00:00",
    });
    return asReminder ? timerAsReminder(p, now) : p;
  }

  // Parse against the current minute so the preview doesn't churn every second;
  // saving re-parses with the exact time.
  const minute = $derived(Math.floor(app.now / 60_000) * 60_000);
  const parsed = $derived(parseAt(minute));
  const isTimer = $derived(!asReminder && parsed?.schedule?.kind === "timer");

  function draftOf(p: Parsed, tag: string | null = p.tagId): ItemDraft | null {
    if (!p.schedule) return null;
    return { title: p.title, notes: "", tag, schedule: p.schedule, nag: p.nag, chime: null, quietExempt: null, startTimer: true };
  }

  // Ask the engine to validate and list the next rings; debounced, stale replies dropped.
  let seq = 0;
  $effect(() => {
    const p = parsed;
    const id = ++seq;
    preview = null;
    previewError = null;
    const draft = p?.title ? draftOf(p) : null;
    if (!draft) return;
    const t = setTimeout(() => {
      api
        .previewSchedule(draft)
        .then((next) => id === seq && (preview = next))
        .catch((e) => id === seq && (previewError = errorText(e)));
    }, 150);
    return () => clearTimeout(t);
  });

  const summary = $derived.by(() => {
    const p = parsed;
    if (!p) return null;
    const s = p.schedule;
    if (!s) return p.title ? "No time yet · Enter to pick one" : null;
    if (s.kind === "timer") return `Timer · ${formatShort(s.durationMs)}`;
    if (s.kind === "oneOff") return `Reminder · ${when(s.due, app.now)}`;
    return `${ruleSummary(s.recurrence.rule)}${s.mode === "afterCompletion" ? " · after you finish" : ""}`;
  });

  const tagName = $derived.by(() => {
    const p = parsed;
    if (!p) return null;
    if (p.unknownTag) return `#${p.unknownTag} (new)`;
    const t = app.tag(p.tagId);
    return t ? `#${t.name}` : null;
  });

  async function resolveTag(p: Parsed): Promise<string | null> {
    if (!p.unknownTag || !app.snapshot) return p.tagId;
    // "#garden" for a tag that doesn't exist yet creates it.
    const count = app.snapshot.tags.length;
    return (await app.run(() => api.saveTag(null, p.unknownTag!, TAG_COLORS[count % TAG_COLORS.length]!, count))) ?? null;
  }

  async function submit(openForm: boolean) {
    const p = parseAt(app.now);
    if (!p?.title || busy) return;
    busy = true;
    try {
      const tag = await resolveTag(p);
      const draft = draftOf(p, tag);
      if (openForm || !draft) {
        onmore({ title: p.title, tag, nag: p.nag, startTimer: true, ...(p.schedule ? { schedule: p.schedule } : {}) });
        clear();
        return;
      }
      const id = await app.run(() => api.createItem(draft));
      if (id !== undefined) {
        clear();
        onadded?.(p.title);
      }
    } finally {
      busy = false;
    }
  }
</script>

<div class="quick">
  <div class="field-row">
    <Icon name="plus" size={18} />
    <!-- svelte-ignore a11y_autofocus -->
    <input
      bind:this={input}
      bind:value={text}
      {placeholder}
      {autofocus}
      aria-label="Quick add"
      aria-describedby="quick-preview"
      disabled={busy}
      oninput={() => (asReminder = false)}
      onkeydown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          submit(e.shiftKey);
        } else if (e.key === "Escape") {
          if (text) clear();
          else onescape?.();
        }
      }}
    />
    {#if parsed?.title}
      <button class="btn btn-quiet more" onclick={() => submit(true)} title="Open the full form (Shift+Enter)">More</button>
    {/if}
  </div>
  <div id="quick-preview" class="preview" aria-live="polite">
    {#if parsed && summary}
      <div class="line" class:error={previewError}>
        <span class="what"><strong>{parsed.title || "…"}</strong> · {summary}{#if tagName} · {tagName}{/if}{#if parsed.nag} · {nagSummary(parsed.nag).toLowerCase()}{/if}</span>
        {#if parsed.schedule?.kind === "timer" || asReminder}
          <button class="link" onclick={() => (asReminder = !asReminder)}>
            {isTimer ? "Reminder instead" : "Timer instead"}
          </button>
        {/if}
      </div>
      {#if previewError}
        <div class="line error">{previewError}</div>
      {:else if preview?.length && parsed.schedule?.kind === "recurring"}
        <div class="line muted">Next {preview.map((t) => when(t, app.now)).join(", ")}</div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .quick {
    display: grid;
    gap: 0.3rem;
  }
  .field-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0 0.4rem 0 0.8rem;
    border-radius: var(--radius);
    background: var(--bg-raised);
    border: 1px solid var(--border);
    color: var(--fg-muted);
  }
  .field-row:focus-within {
    border-color: var(--focus);
  }
  input {
    flex: 1;
    min-width: 0;
    min-height: 2.6rem;
    border: none;
    outline: none;
    background: transparent;
    color: var(--fg);
  }
  .more {
    min-height: 1.9rem;
    padding: 0.2rem 0.7rem;
    font-size: 0.85rem;
  }
  .preview {
    display: grid;
    gap: 0.05rem;
    padding: 0 0.5rem;
    font-size: 0.83rem;
  }
  .preview:empty {
    display: none;
  }
  .line {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    min-width: 0;
  }
  .what {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .link {
    flex: none;
    padding: 0;
    border: none;
    background: none;
    color: var(--focus);
    font-size: inherit;
    cursor: pointer;
  }
  .error {
    color: var(--ringing);
  }
</style>
