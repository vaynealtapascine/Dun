<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import type { ItemDraft, ItemView } from "./lib/api/types";
  import { api } from "./lib/api/commands";
  import { filterItems } from "./lib/grouping";
  import { when } from "./lib/format";
  import { app } from "./lib/stores/app.svelte";
  import Icon from "./lib/components/Icon.svelte";
  import QuickAdd from "./lib/components/QuickAdd.svelte";
  import TagDot from "./lib/components/TagDot.svelte";
  import AlarmSpike from "./views/AlarmSpike.svelte";
  import History from "./views/History.svelte";
  import ItemForm from "./views/ItemForm.svelte";
  import Reminders from "./views/Reminders.svelte";
  import Settings from "./views/Settings.svelte";
  import Timers from "./views/Timers.svelte";

  type Tab = "reminders" | "timers" | "history";

  // The Android build still runs the alarm spike until the phone app lands.
  const isAndroid = navigator.userAgent.includes("Android") && !("__DUN_PREVIEW__" in window);

  let tab = $state<Tab>("reminders");
  let showSettings = $state(false);
  let query = $state("");
  let tagFilter = $state<string | null>(null);
  let formOpen = $state(false);
  let editing = $state<ItemView | null>(null);
  let prefill = $state<Partial<ItemDraft> | null>(null);

  $effect(() => {
    if (isAndroid) return;
    app.start();
    const focus = listen<string>("focus-item", (e) => {
      const item = app.snapshot?.items.find((i) => i.id === e.payload);
      if (item) edit(item);
    });
    return () => {
      app.stop();
      focus.then((f) => f());
    };
  });

  const snapshot = $derived(app.snapshot);
  const visible = $derived(snapshot ? filterItems(snapshot.items, query, tagFilter, snapshot.tags) : []);
  const ringingCount = $derived(
    snapshot?.items.filter((i) => i.status.kind === "due" && !i.status.snoozed && i.ring?.held == null).length ?? 0,
  );
  const mutedUntil = $derived(
    snapshot?.settings.muteUntil != null && snapshot.settings.muteUntil > app.now ? snapshot.settings.muteUntil : null,
  );

  function edit(item: ItemView) {
    editing = item;
    prefill = null;
    formOpen = true;
  }

  function add(draft: Partial<ItemDraft> | null = null) {
    editing = null;
    prefill = draft;
    formOpen = true;
  }
</script>

{#if isAndroid}
  <main class="spike"><h1>Dun</h1><AlarmSpike /></main>
{:else}
  <div class="app">
    <header>
      {#if showSettings}
        <button class="icon-btn" aria-label="Back" onclick={() => (showSettings = false)}>
          <Icon name="close" />
        </button>
        <h1>Settings</h1>
        <span class="spacer"></span>
      {:else}
        <h1>Dun</h1>
        {#if mutedUntil}
          <button class="pill" onclick={() => app.run(() => api.unmute())} title="Unmute">
            <Icon name="bellOff" size={14} /> Muted until {when(mutedUntil, app.now)}
          </button>
        {/if}
        <span class="spacer"></span>
        <button class="icon-btn" aria-label="Settings" onclick={() => (showSettings = true)}>
          <Icon name="gear" />
        </button>
      {/if}
    </header>

    {#if showSettings}
      <main><Settings /></main>
    {:else}
      <div class="toolbar">
        {#if tab !== "history"}
          <QuickAdd onmore={add} onadded={(title) => app.notify(`Added “${title}”`)} />
        {/if}
        <label class="search">
          <Icon name="search" size={16} />
          <span class="sr-only">Search</span>
          <input type="search" placeholder="Search" bind:value={query} />
        </label>
        {#if (snapshot?.tags.length ?? 0) > 0}
          <div class="tags" role="group" aria-label="Filter by tag">
            <button class="chip" aria-pressed={tagFilter === null} onclick={() => (tagFilter = null)}>All</button>
            {#each snapshot?.tags ?? [] as t (t.id)}
              <button
                class="chip"
                aria-pressed={tagFilter === t.id}
                onclick={() => (tagFilter = tagFilter === t.id ? null : t.id)}
              >
                <TagDot color={t.color} />{t.name}
              </button>
            {/each}
          </div>
        {/if}
        <div class="segmented" role="tablist">
          <button role="tab" aria-selected={tab === "reminders"} onclick={() => (tab = "reminders")}>
            Reminders {#if ringingCount > 0}<span class="badge">{ringingCount}</span>{/if}
          </button>
          <button role="tab" aria-selected={tab === "timers"} onclick={() => (tab = "timers")}>Timers</button>
          <button role="tab" aria-selected={tab === "history"} onclick={() => (tab = "history")}>History</button>
        </div>
      </div>

      <main>
        {#if !snapshot}
          <p class="muted loading">Loading…</p>
        {:else if tab === "reminders"}
          <Reminders items={visible} onedit={edit} />
        {:else if tab === "timers"}
          <Timers items={visible} onedit={edit} />
        {:else}
          <History />
        {/if}
      </main>

      {#if tab !== "history"}
        <button class="fab" aria-label="Add" onclick={() => add()}><Icon name="plus" size={26} /></button>
      {/if}
    {/if}

    {#if app.notice && !app.error}
      <div class="notice" role="status">{app.notice}</div>
    {/if}

    {#if app.error}
      <div class="error" role="alert">
        <span>{app.error}</span>
        <button class="icon-btn" aria-label="Dismiss" onclick={() => (app.error = null)}><Icon name="close" size={16} /></button>
      </div>
    {/if}
  </div>

  <ItemForm bind:open={formOpen} item={editing} {prefill} initialKind={tab === "timers" ? "timer" : "once"} />
{/if}

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: max(0.6rem, env(safe-area-inset-top)) 0.75rem 0.2rem 1rem;
  }
  h1 {
    margin: 0;
    font-size: 1.5rem;
    letter-spacing: -0.02em;
  }
  .spacer {
    flex: 1;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.2rem 0.6rem;
    border-radius: 999px;
    border: none;
    background: var(--overdue-bg);
    color: var(--overdue);
    font-size: 0.8rem;
    font-weight: 600;
    cursor: pointer;
  }
  .toolbar {
    display: grid;
    gap: 0.5rem;
    padding: 0.3rem 0.75rem 0.6rem;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0 0.7rem;
    border-radius: 999px;
    background: var(--bg-sunken);
    color: var(--fg-muted);
  }
  .search input {
    flex: 1;
    min-height: 2.2rem;
    border: none;
    outline: none;
    background: transparent;
    color: var(--fg);
  }
  .tags {
    display: flex;
    gap: 0.35rem;
    overflow-x: auto;
    padding-bottom: 0.1rem;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.25rem 0.7rem;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--bg-raised);
    font-size: 0.85rem;
    white-space: nowrap;
    cursor: pointer;
  }
  .chip[aria-pressed="true"] {
    border-color: var(--fg);
    font-weight: 600;
  }
  .badge {
    display: inline-grid;
    place-items: center;
    min-width: 1.2rem;
    height: 1.2rem;
    padding: 0 0.3rem;
    margin-left: 0.2rem;
    border-radius: 999px;
    background: var(--ringing);
    color: #fff;
    font-size: 0.72rem;
    font-weight: 700;
  }
  main {
    flex: 1;
    overflow-y: auto;
    padding: 0.2rem 0.75rem 5.5rem;
  }
  .loading {
    text-align: center;
    padding: 3rem;
  }
  .fab {
    position: fixed;
    right: 1.1rem;
    bottom: max(1.1rem, env(safe-area-inset-bottom));
    display: grid;
    place-items: center;
    width: 3.5rem;
    height: 3.5rem;
    border-radius: 999px;
    border: none;
    background: var(--accent);
    color: var(--accent-fg);
    box-shadow: var(--shadow);
    cursor: pointer;
  }
  .fab:hover {
    filter: brightness(1.06);
  }
  .error {
    position: fixed;
    left: 0.75rem;
    right: 5rem;
    bottom: 1.1rem;
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.4rem 0.4rem 0.4rem 0.9rem;
    border-radius: var(--radius);
    background: var(--ringing);
    color: #fff;
    box-shadow: var(--shadow);
  }
  .error span {
    flex: 1;
  }
  .error .icon-btn {
    color: #fff;
  }
  .notice {
    position: fixed;
    left: 0.75rem;
    bottom: 1.6rem;
    max-width: calc(100% - 6rem);
    padding: 0.5rem 0.9rem;
    border-radius: var(--radius);
    background: var(--fg);
    color: var(--bg);
    box-shadow: var(--shadow);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .spike {
    padding: max(2rem, env(safe-area-inset-top)) 1rem 2rem;
  }
</style>
