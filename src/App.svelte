<script lang="ts">
  import { tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import type { ItemDraft, ItemView } from "./lib/api/types";
  import { api } from "./lib/api/commands";
  import { filterItems } from "./lib/grouping";
  import { when } from "./lib/format";
  import { isPhone } from "./lib/platform";
  import { startForegroundSync } from "./lib/phoneSync";
  import { enterFrom, ms, reducedMotion, reveal } from "./lib/motion";
  import { wheelScrollsX } from "./lib/gestures";
  import { fade, fly } from "svelte/transition";
  import { app } from "./lib/stores/app.svelte";
  import Icon from "./lib/components/Icon.svelte";
  import QuickAdd from "./lib/components/QuickAdd.svelte";
  import TagDot from "./lib/components/TagDot.svelte";
  import History from "./views/History.svelte";
  import ItemForm from "./views/ItemForm.svelte";
  import Reminders from "./views/Reminders.svelte";
  import Settings from "./views/Settings.svelte";
  import Timers from "./views/Timers.svelte";

  type Tab = "reminders" | "timers" | "history";

  const TABS: Tab[] = ["timers", "reminders", "history"];

  let tab = $state<Tab>("timers");
  /** Which way the last tab change went, so the new panel arrives from that side. */
  let direction = $state(0);
  let mainEl = $state<HTMLElement>();
  /** Each tab keeps its own place in the list. */
  const scrollTops: Record<Tab, number> = { timers: 0, reminders: 0, history: 0 };
  /** The add button tucks its label away while the list scrolls down. */
  let fabCompact = $state(false);
  let lastScroll = 0;
  let showSettings = $state(false);
  let showTools = $state(false);
  let query = $state("");
  let tagFilter = $state<string | null>(null);
  let formOpen = $state(false);
  let editing = $state<ItemView | null>(null);
  let prefill = $state<Partial<ItemDraft> | null>(null);
  let quickAdd = $state<QuickAdd>();
  let searchInput = $state<HTMLInputElement>();

  function shortcuts(e: KeyboardEvent) {
    if (document.querySelector("dialog[open]")) return;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      if (!showSettings && tab !== "history") {
        tick().then(() => quickAdd?.focus());
      }
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f" && !showSettings) {
      e.preventDefault();
      showTools = true;
      tick().then(() => searchInput?.focus());
    }
    if (e.key === "Escape") { query = ""; tagFilter = null; showSettings = false; showTools = false; }
  }

  $effect(() => {
    document.documentElement.dataset.theme = app.local?.theme ?? "system";
  });

  $effect(() => {
    app.start();
    // The phone's alarms sync before every alert; this keeps the list honest
    // in between, while someone is actually looking at it.
    const stopSync = isPhone ? startForegroundSync() : undefined;
    const focus = listen<string>("focus-item", (e) => {
      const item = app.snapshot?.items.find((i) => i.id === e.payload);
      if (item) edit(item);
    });
    // "More" in the quick-add window continues here.
    const openForm = listen<Partial<ItemDraft>>("open-form", (e) => add(e.payload));
    return () => {
      app.stop();
      stopSync?.();
      focus.then((f) => f());
      openForm.then((f) => f());
    };
  });

  const snapshot = $derived(app.snapshot);
  const visible = $derived(snapshot ? filterItems(snapshot.items, query, tagFilter, snapshot.tags) : []);
  const ringingCount = $derived(
    snapshot?.items.filter((i) => i.kind !== "timer" && i.status.kind === "due" && !i.status.snoozed && i.ring?.held == null).length ?? 0,
  );
  const timersRingingCount = $derived(snapshot?.items.filter((i) => i.kind === "timer" && i.status.kind === "due" && !i.status.snoozed && i.ring?.held == null).length ?? 0);
  $effect(() => {
    document.documentElement.dataset.timerOverdue = String(timersRingingCount > 0);
    return () => { delete document.documentElement.dataset.timerOverdue; };
  });
  const mutedUntil = $derived(
    snapshot?.settings.muteUntil != null && snapshot.settings.muteUntil > app.now ? snapshot.settings.muteUntil : null,
  );

  /** New counts arrive with a small pop, so a tab that starts ringing is noticed. */
  function badgeIn(_node: Element) {
    return {
      duration: ms(260),
      css: (t: number) => `transform: scale(${0.4 + 0.6 * (1 - Math.pow(1 - t, 3)) + Math.sin(t * Math.PI) * 0.12}); opacity: ${Math.min(1, t * 2)};`,
    };
  }

  function selectTab(next: Tab) {
    if (next === tab) {
      // A second tap on the current tab goes back to the top, as on most phones.
      mainEl?.scrollTo({ top: 0, behavior: reducedMotion() ? "auto" : "smooth" });
      return;
    }
    if (mainEl) scrollTops[tab] = mainEl.scrollTop;
    direction = Math.sign(TABS.indexOf(next) - TABS.indexOf(tab));
    tab = next;
    fabCompact = false;
    tick().then(() => {
      if (mainEl) mainEl.scrollTop = scrollTops[next];
      lastScroll = mainEl?.scrollTop ?? 0;
    });
  }

  function onscroll() {
    const top = mainEl?.scrollTop ?? 0;
    if (Math.abs(top - lastScroll) < 12) return;
    fabCompact = top > lastScroll && top > 48;
    lastScroll = top;
  }

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
<svelte:window onkeydown={shortcuts} />

<div class="app">
  <header class:settings-header={showSettings}>
    {#if showSettings}
      <button class="icon-btn" aria-label="Back" onclick={() => (showSettings = false)}><Icon name="close" /></button>
      <h1>Settings</h1>
      <span class="spacer"></span>
    {:else}
      <h1>Dun</h1>
      <span class="spacer"></span>
      {#if mutedUntil}
        <button class="mute-status" onclick={() => app.run(() => api.unmute())} title="Unmute">
          <Icon name="bellOff" size={16} /><span>Muted until {when(mutedUntil, app.now)}</span>
        </button>
      {/if}
      <button class="icon-btn" class:tools-active={showTools || query || tagFilter} aria-label="Search and filter" aria-expanded={showTools} aria-controls="filter-tools" onclick={() => {
        showTools = !showTools;
        if (showTools) tick().then(() => searchInput?.focus());
      }}><Icon name="search" size={21} /></button>
      <button class="icon-btn settings-button" aria-label="Settings" onclick={() => (showSettings = true)}><Icon name="gear" size={24} /></button>
    {/if}
  </header>

  {#if showSettings}
    <main class="settings-main" in:enterFrom={{ x: 24 }}><Settings /></main>
  {:else}
    <div class="tabs" role="tablist" tabindex="-1" aria-label="Items" onkeydown={(e) => {
      if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) return;
      e.preventDefault();
      const index = e.key === "Home" ? 0 : e.key === "End" ? 2 : (TABS.indexOf(tab) + (e.key === "ArrowRight" ? 1 : 2)) % 3;
      selectTab(TABS[index]!);
      (e.currentTarget.children[index] as HTMLElement).focus();
    }}>
      <button id="tab-timers" role="tab" tabindex={tab === "timers" ? 0 : -1} aria-controls="items-panel" aria-selected={tab === "timers"} onclick={() => selectTab("timers")}>Timers {#if timersRingingCount > 0 && tab !== "timers"}<span class="badge" in:badgeIn>{timersRingingCount}</span>{/if}</button>
      <button id="tab-reminders" role="tab" tabindex={tab === "reminders" ? 0 : -1} aria-controls="items-panel" aria-selected={tab === "reminders"} onclick={() => selectTab("reminders")}>Reminders {#if ringingCount > 0 && tab !== "reminders"}<span class="badge" in:badgeIn>{ringingCount}</span>{/if}</button>
      <button id="tab-history" role="tab" tabindex={tab === "history" ? 0 : -1} aria-controls="items-panel" aria-selected={tab === "history"} onclick={() => selectTab("history")}>History</button>
      <span class="indicator" aria-hidden="true" style:--tab-index={TABS.indexOf(tab)}></span>
    </div>

    {#if showTools}
      <div class="toolbar" id="filter-tools" transition:reveal>
        <div class="search-row">
          <label class="search">
            <Icon name="search" size={18} /><span class="sr-only">Search</span>
            <input bind:this={searchInput} type="search" placeholder="Search {tab}" bind:value={query} />
          </label>
        </div>
        {#if (snapshot?.tags.length ?? 0) > 0}
          <div class="tags" role="group" aria-label="Filter by tag" use:wheelScrollsX>
            <button class="chip" aria-pressed={tagFilter === null} onclick={() => (tagFilter = null)}>All</button>
            {#each snapshot?.tags ?? [] as t (t.id)}
              <button class="chip" aria-pressed={tagFilter === t.id} onclick={() => (tagFilter = tagFilter === t.id ? null : t.id)}><TagDot color={t.color} />{t.name}</button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
    {#if tab !== "history"}
      <div class="capture">
        <QuickAdd bind:this={quickAdd} onmore={add} onadded={(title) => app.notify(`Added “${title}”`)} />
      </div>
    {/if}

    <main class:reminder-panel={tab === "reminders"} bind:this={mainEl} {onscroll}>
      {#key tab}
      <div id="items-panel" role="tabpanel" aria-labelledby={"tab-" + tab} tabindex="-1" in:enterFrom={{ x: direction * 24 }}>
      {#if !snapshot}
        <p class="muted loading" role="status">Loading timers and reminders…</p>
      {:else if tab !== "history" && (query || tagFilter) && !visible.some((i) => tab === "timers" ? i.kind === "timer" : i.kind !== "timer" && i.status.kind !== "idle")}
        <div class="empty-results">
          <h2>No matching {tab}</h2>
          <p class="muted">Try another search or clear your filters.</p>
          <button class="btn" onclick={() => { query = ""; tagFilter = null; }}>Clear filters</button>
        </div>
      {:else if tab === "reminders"}
        <Reminders items={visible} onedit={edit} />
      {:else if tab === "timers"}
        <Timers items={visible} onedit={edit} />
      {:else}
        <History {query} />
      {/if}
      </div>
      {/key}
    </main>

    {#if tab === "reminders"}
      <button class="fab" class:compact={fabCompact} aria-label="Add reminder" onclick={() => add()} transition:fly={{ y: 24, duration: ms(200) }}>
        <Icon name="plus" size={24} /><span class="fab-label">Add reminder</span>
      </button>
    {/if}
  {/if}

  {#if app.notice && !app.error}
    <div class="notice" role="status" in:fly={{ y: 16, duration: ms(220) }} out:fade={{ duration: ms(160) }}>
      <span>{app.notice}</span>
      {#if app.noticeUndo}<button class="btn" onclick={() => app.run(() => app.noticeUndo!())}>Undo</button>{/if}
    </div>
  {/if}
  {#if app.error}
    <div class="error" role="alert" in:fly={{ y: 16, duration: ms(220) }} out:fade={{ duration: ms(160) }}>
      <span>{app.error}</span>
      <button class="icon-btn" aria-label="Dismiss" onclick={() => (app.error = null)}><Icon name="close" size={18} /></button>
    </div>
  {/if}
</div>
<ItemForm bind:open={formOpen} item={editing} {prefill} initialKind={tab === "timers" ? "timer" : "once"} />

<style>
  .app { display: flex; flex-direction: column; height: 100dvh; max-width: var(--app-width); margin-inline: auto; }
  header { display: flex; align-items: center; gap: 0.25rem; padding: env(safe-area-inset-top) calc(var(--page-gutter, 1rem) - 0.35rem) 0 var(--page-gutter, 1rem); min-height: max(var(--hit), 2.55rem); }
  header .icon-btn { width: max(var(--hit), 2.55rem); height: max(var(--hit), 2.55rem); }
  h1 { margin: 0; font-size: 1.75rem; font-weight: 700; line-height: 1; letter-spacing: -0.025em; }
  .settings-header h1 { font-size: 1.6rem; }
  .settings-button { color: var(--fg); }
  .spacer { flex: 1; }
  .mute-status { display: inline-flex; align-items: center; gap: 0.35rem; min-height: 36px; padding: 0.3rem 0.55rem; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--overdue-bg); color: var(--overdue); cursor: pointer; font-size: 0.8rem; max-width: 50%; }
  .mute-status span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tools-active { color: var(--accent); background: var(--bg-sunken); }
  .tabs { position: relative; display: flex; padding: 0 var(--page-gutter, 0.75rem); border-top: 1px solid var(--border); border-bottom: 1px solid var(--border); }
  .tabs button { flex: 1; position: relative; min-height: var(--hit); padding: 0.45rem 0.25rem; border: none; background: transparent; color: var(--fg-muted); cursor: pointer; font-size: 1rem; font-weight: 600; transition: color var(--dur) var(--ease-out), background-color var(--dur-fast) ease-out; }
  .tabs button[aria-selected="true"] { color: var(--accent); font-weight: 700; }
  /* One underline travels between tabs rather than three blinking on and off. */
  .indicator { position: absolute; left: var(--page-gutter, 0.75rem); bottom: 0; width: calc((100% - 2 * var(--page-gutter, 0.75rem)) / 3); height: 0.25rem; padding-inline: 0.25rem; background-clip: content-box; border-radius: 4px 4px 0 0; background-color: var(--accent); transform: translateX(calc(var(--tab-index) * 100%)); transition: transform var(--dur-slow) var(--ease-out); pointer-events: none; }
  @media (hover: hover) { .tabs button:hover { background: var(--bg-sunken); } }
  .tabs button:active { background: var(--bg-sunken); }
  .badge { position: absolute; right: 0.7rem; top: 50%; transform: translateY(-50%); display: inline-grid; place-items: center; min-width: 1.15rem; height: 1.15rem; padding: 0 0.25rem; border-radius: 4px; background: var(--alarm); color: var(--alarm-fg); font-size: 0.75rem; font-weight: 700; }
  @media (max-width: 600px) {
    .tabs button { font-size: 0.9rem; }
    .badge { position: static; transform: none; min-width: 0.9rem; height: 0.9rem; font-size: 0.65rem; padding-inline: 0.15rem; margin-left: 0.25rem; vertical-align: middle; }
  }
  @media (min-width: 760px) and (max-aspect-ratio: 3/4) { .settings-button :global(svg) { width: 1.75rem; height: 1.75rem; } }
  .toolbar { display: grid; gap: 0.55rem; padding: 0.75rem var(--page-gutter, 0.75rem) 0; }
  .search-row { display: flex; gap: 0.5rem; }
  .search { display: flex; flex: 1; min-width: 0; align-items: center; gap: 0.5rem; padding: 0 0.7rem; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg-raised); color: var(--fg-muted); }
  .search:focus-within { outline: 2px solid var(--focus); outline-offset: 2px; }
  .search input { width: 100%; min-width: 0; min-height: var(--hit); padding: 0; border: none; outline: none; background: transparent; color: var(--fg); }
  .tags { display: flex; gap: 0.4rem; overflow-x: auto; padding: 0.15rem 0 0.3rem; }
  .chip { display: inline-flex; align-items: center; gap: 0.35rem; min-height: 36px; padding: 0.3rem 0.7rem; border-radius: var(--radius-sm); border: 1px solid var(--border); background: var(--bg-raised); white-space: nowrap; cursor: pointer; }
  .chip { transition: background-color var(--dur-fast) ease-out, color var(--dur-fast) ease-out, border-color var(--dur-fast) ease-out, transform var(--dur-fast) var(--ease-out); }
  .chip:active { transform: scale(0.96); }
  .chip[aria-pressed="true"] { background: var(--accent); border-color: var(--accent); color: var(--accent-fg); font-weight: 700; }
  .capture { padding: 0.75rem var(--page-gutter, 0.75rem) 0; }
  main { flex: 1; min-height: 0; overflow-y: auto; overflow-x: clip; padding: 0.5rem var(--page-gutter, 0.625rem) max(0.4rem, env(safe-area-inset-bottom)); }
  .reminder-panel { padding-bottom: 5.5rem; }
  .settings-main { border-top: 1px solid var(--border); }
  .loading { text-align: center; padding: 3rem 1rem; }
  .empty-results { text-align: center; padding: 3rem 1rem; }
  .empty-results h2 { margin: 0; font-size: 1.6rem; }
  .empty-results p { margin: 0.5rem 0 1.25rem; }
  .fab { position: fixed; right: max(1rem, calc((100vw - var(--app-width)) / 2 + 1rem)); bottom: max(1rem, env(safe-area-inset-bottom)); display: flex; align-items: center; justify-content: center; min-height: 48px; min-width: 48px; padding: 0.5rem 1rem 0.5rem 0.75rem; border-radius: var(--radius); border: none; background: var(--accent); color: var(--accent-fg); font-weight: 700; box-shadow: var(--shadow); cursor: pointer; transition: padding var(--dur-slow) var(--ease-out), transform var(--dur-fast) var(--ease-out), background-color var(--dur-fast) ease-out; }
  .fab-label { display: inline-block; max-width: 10rem; margin-left: 0.5rem; overflow: hidden; white-space: nowrap; transition: max-width var(--dur-slow) var(--ease-out), margin var(--dur-slow) var(--ease-out), opacity var(--dur) ease-out; }
  .fab.compact { padding-inline: 0.75rem; }
  .fab.compact .fab-label { max-width: 0; margin-left: 0; opacity: 0; }
  @media (hover: hover) { .fab:hover { background: color-mix(in srgb, var(--accent) 88%, var(--fg)); } }
  .fab:active { transform: scale(0.96); }
  .error, .notice { position: fixed; z-index: 20; left: max(0.75rem, calc((100vw - var(--app-width)) / 2 + 0.75rem)); bottom: max(1rem, env(safe-area-inset-bottom)); display: flex; align-items: center; gap: 0.6rem; max-width: min(90vw, calc(var(--app-width) - 2rem)); padding: 0.5rem 0.8rem; border-radius: var(--radius); box-shadow: var(--shadow); }
  .error { right: max(0.75rem, calc((100vw - var(--app-width)) / 2 + 0.75rem)); background: var(--alarm); color: var(--alarm-fg); }
  .error span { flex: 1; }
  .error .icon-btn { color: inherit; }
  .notice { background: var(--fg); color: var(--bg); }
  .notice span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .notice .btn { flex: none; color: inherit; background: transparent; border-color: currentColor; }
  @media (max-width: 380px) { .mute-status span { max-width: 4rem; } .search-row { flex-wrap: wrap; } }
</style>
