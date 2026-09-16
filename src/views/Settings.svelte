<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import type { ChimeOption, ChimeRef, LocalSettings, Nag, QuietHours, SyncStatus } from "../lib/api/types";
  import { api } from "../lib/api/commands";
  import { toCivilTime, toTimeInput } from "../lib/dates";
  import { when } from "../lib/format";
  import { app } from "../lib/stores/app.svelte";
  import { isDesktop } from "../lib/platform";
  import HotkeyInput from "../lib/components/HotkeyInput.svelte";
  import PhoneSettings from "./PhoneSettings.svelte";
  import Pairing from "./Pairing.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import TagDot from "../lib/components/TagDot.svelte";

  const TAG_COLORS = ["#e0483e", "#f08c2e", "#e8b930", "#3cb371", "#2fa4c9", "#3a86ff", "#8a63d2", "#d05ea0", "#8d8d93"];
  const NAG_INTERVALS = [1, 2, 3, 5, 10, 15, 30, 60];

  const settings = $derived(app.snapshot?.settings);
  const local = $derived(app.local);
  let chimes = $state<ChimeOption[]>([]);
  let newTag = $state("");
  let version = $state("");
  let hotkeyError = $state<string | null>(null);
  let sync = $state<SyncStatus | null>(null);
  let syncError = $state<string | null>(null);
  let pairingOpen = $state(false);

  const DEFAULT_HOTKEY = "CommandOrControl+Alt+N";

  $effect(() => {
    api.chimes().then((c) => (chimes = c)).catch(() => {});
    invoke<string>("app_version").then((v) => (version = v)).catch(() => {});
    api.hotkeyStatus().then((s) => (hotkeyError = s)).catch(() => {});
    api.syncStatus().then((s) => (sync = s)).catch(() => {});
    const changed = listen<SyncStatus>("sync-changed", (e) => (sync = e.payload));
    return () => changed.then((f) => f());
  });

  function setQuiet(patch: Partial<QuietHours>) {
    if (!settings) return;
    app.run(() => api.setSetting("quietHours", { ...settings.quietHours, ...patch }));
  }

  function setNag(nag: Nag) {
    app.run(() => api.setSetting("nagDefault", nag));
  }

  async function setLocal(patch: Partial<LocalSettings>) {
    if (!local) return;
    const previous = local;
    const next = { ...local, ...patch };
    app.local = next;
    await app.run(() => api.setLocalSettings(next));
    // Refused (e.g. a shortcut another app owns): show what's really saved.
    if (app.error) app.local = previous;
    else if (patch.hotkey !== undefined) hotkeyError = null;
  }

  async function importSound() {
    const chime = await app.run(() => api.importSound());
    if (!chime) return;
    chimes = await api.chimes();
    setLocal({ chime });
    app.notify(`Added ${chime.kind === "custom" ? chime.name : chime.id}`);
    api.playChime(chime);
  }

  async function exportBackup() {
    const path = await app.run(() => api.backupExport());
    if (path) app.notify(`Saved to ${path}`);
  }

  async function importBackup(mode: "merge" | "replace") {
    const report = await app.run(() => api.backupImport(mode));
    if (!report) return; // cancelled
    const parts = [`${report.registersChanged} changes`];
    if (report.historyAdded) parts.push(`${report.historyAdded} history entries`);
    if (report.deleted) parts.push(`${report.deleted} removed`);
    app.notify(`${mode === "merge" ? "Merged" : "Replaced"}: ${parts.join(", ")}`);
  }

  async function setSyncEnabled(enabled: boolean, skipFirewall = false) {
    syncError = null;
    try {
      await api.syncSetEnabled(enabled, skipFirewall);
      sync = await api.syncStatus();
    } catch (e) {
      syncError = String(e);
    }
  }

  async function forget(deviceId: string, name: string) {
    if (!confirm(`Forget ${name}? It will stop syncing until you pair it again.`)) return;
    await app.run(() => api.forgetPeer(deviceId));
    sync = await api.syncStatus();
  }

  const chimeKey = (c: ChimeRef) => (c.kind === "bundled" ? `b:${c.id}` : `c:${c.sha256}`);
  const muted = $derived(settings?.muteUntil != null && settings.muteUntil > app.now);
</script>

{#if settings && local}
  <div class="settings">
    <section class="card">
      <h3>Nagging</h3>
      <label class="switch">
        <span>New items keep nagging until done</span>
        <input
          type="checkbox"
          checked={settings.nagDefault.mode === "repeat"}
          onchange={(e) =>
            setNag(e.currentTarget.checked ? { mode: "repeat", intervalMin: 1 } : { mode: "once" })}
        />
      </label>
      {#if settings.nagDefault.mode === "repeat"}
        <label class="field">
          <span>Default nag interval</span>
          <select
            class="input"
            value={settings.nagDefault.intervalMin}
            onchange={(e) => setNag({ mode: "repeat", intervalMin: Number(e.currentTarget.value) })}
          >
            {#each NAG_INTERVALS as m (m)}
              <option value={m}>{m === 1 ? "Every minute" : `Every ${m} minutes`}</option>
            {/each}
          </select>
        </label>
      {/if}
      <label class="field">
        <span>Group missed reminders when more than</span>
        <input
          class="input narrow"
          type="number"
          min="1"
          max="50"
          value={settings.missedSummaryThreshold}
          onchange={(e) =>
            app.run(() => api.setSetting("missedSummaryThreshold", Math.max(1, Number(e.currentTarget.value) || 3)))}
        />
      </label>
    </section>

    <section class="card">
      <h3>Quiet hours</h3>
      <label class="switch">
        <span>Hold reminders overnight</span>
        <input type="checkbox" checked={settings.quietHours.enabled} onchange={(e) => setQuiet({ enabled: e.currentTarget.checked })} />
      </label>
      {#if settings.quietHours.enabled}
        <div class="pair">
          <label class="field">
            <span>From</span>
            <input class="input" type="time" value={toTimeInput(settings.quietHours.start)} onchange={(e) => setQuiet({ start: toCivilTime(e.currentTarget.value) })} />
          </label>
          <label class="field">
            <span>Until</span>
            <input class="input" type="time" value={toTimeInput(settings.quietHours.end)} onchange={(e) => setQuiet({ end: toCivilTime(e.currentTarget.value) })} />
          </label>
        </div>
        <p class="hint muted">Timers still ring unless you change it on the timer.</p>
      {/if}
    </section>

    <section class="card">
      <h3>Mute</h3>
      {#if muted}
        <p class="status">Muted until {when(settings.muteUntil!, app.now)}</p>
      {/if}
      <div class="buttons">
        <button class="btn" onclick={() => app.run(() => api.muteFor(15))}>15 min</button>
        <button class="btn" onclick={() => app.run(() => api.muteFor(60))}>1 hour</button>
        <button class="btn" onclick={() => app.run(() => api.muteUntilTomorrow())}>Until tomorrow</button>
        {#if muted}<button class="btn btn-primary" onclick={() => app.run(() => api.unmute())}>Unmute</button>{/if}
      </div>
    </section>

    {#if isDesktop}
    <section class="card">
      <h3>Sound <span class="scope">this device</span></h3>
      <div class="field">
        <span>Chime</span>
        <div class="inline">
          <select
            class="input"
            value={chimeKey(local.chime)}
            onchange={(e) => {
              const c = chimes.find((o) => chimeKey(o.chime) === e.currentTarget.value);
              if (c) setLocal({ chime: c.chime });
            }}
          >
            {#each chimes as c (chimeKey(c.chime))}
              <option value={chimeKey(c.chime)}>{c.label}</option>
            {/each}
          </select>
          <button class="icon-btn" aria-label="Play" onclick={() => api.playChime(local.chime)}><Icon name="play" size={16} /></button>
          <button class="btn" onclick={importSound}>Import…</button>
        </div>
      </div>
      <label class="field">
        <span>Volume {Math.round(local.volume * 100)}%</span>
        <input
          type="range"
          min="0"
          max="1"
          step="0.05"
          value={local.volume}
          onchange={(e) => {
            setLocal({ volume: Number(e.currentTarget.value) });
            api.playChime(local.chime);
          }}
        />
      </label>
    </section>

    <section class="card">
      <h3>This PC <span class="scope">this device</span></h3>
      <div class="field">
        <span>Quick-add shortcut (works from any app)</span>
        <HotkeyInput value={local.hotkey} defaultValue={DEFAULT_HOTKEY} onchange={(hotkey) => setLocal({ hotkey })} />
        {#if hotkeyError}<p class="hint warn">{hotkeyError}</p>{/if}
      </div>
      <label class="switch">
        <span>Start Dun when I sign in</span>
        <input type="checkbox" checked={local.autostart} onchange={(e) => setLocal({ autostart: e.currentTarget.checked })} />
      </label>
      <label class="field">
        <span>Away from the PC after</span>
        <select class="input" value={local.idleThresholdS} onchange={(e) => setLocal({ idleThresholdS: Number(e.currentTarget.value) })}>
          {#each [60, 120, 300, 600, 900, 1800] as s (s)}
            <option value={s}>{s / 60} minutes idle</option>
          {/each}
        </select>
      </label>
      <label class="field">
        <span>Appearance</span>
        <div class="segmented" role="group">
          {#each [["system", "System"], ["light", "Light"], ["dark", "Dark"]] as [value, label] (value)}
            <button type="button" aria-pressed={local.theme === value} onclick={() => setLocal({ theme: value as LocalSettings["theme"] })}>{label}</button>
          {/each}
        </div>
      </label>
    </section>

    {:else}
      <PhoneSettings />
    {/if}

    <section class="card">
      <h3>Tags</h3>
      <div class="tags">
        {#each app.snapshot?.tags ?? [] as tag (tag.id)}
          <div class="tag">
            <TagDot color={tag.color} size={12} />
            <input
              class="input"
              value={tag.name}
              onchange={(e) => app.run(() => api.saveTag(tag.id, e.currentTarget.value, tag.color, tag.order))}
            />
            <div class="swatches">
              {#each TAG_COLORS as color (color)}
                <button
                  class="swatch"
                  style:background={color}
                  aria-label="Color {color}"
                  aria-pressed={tag.color === color}
                  onclick={() => app.run(() => api.saveTag(tag.id, tag.name, color, tag.order))}
                ></button>
              {/each}
            </div>
            <button class="icon-btn" aria-label="Delete tag {tag.name}" onclick={() => app.run(() => api.deleteTag(tag.id))}>
              <Icon name="trash" size={16} />
            </button>
          </div>
        {/each}
        <form
          class="inline"
          onsubmit={(e) => {
            e.preventDefault();
            if (!newTag.trim()) return;
            const count = app.snapshot?.tags.length ?? 0;
            app.run(() => api.saveTag(null, newTag, TAG_COLORS[count % TAG_COLORS.length]!, count));
            newTag = "";
          }}
        >
          <input class="input" placeholder="New tag" bind:value={newTag} />
          <button class="btn" type="submit"><Icon name="plus" size={14} /> Add</button>
        </form>
      </div>
    </section>

    {#if isDesktop}
    <section class="card">
      <h3>Sync <span class="scope">this device</span></h3>
      <label class="switch">
        <span>Sync with my phone over the local network</span>
        <input
          type="checkbox"
          checked={sync?.enabled ?? false}
          onchange={(e) => setSyncEnabled(e.currentTarget.checked)}
        />
      </label>
      {#if syncError}
        <p class="hint warn">{syncError}</p>
        <div class="buttons">
          <button class="btn" onclick={() => setSyncEnabled(true, true)}>Turn on anyway</button>
        </div>
      {/if}
      {#if sync?.enabled}
        <p class="hint muted">
          {#if sync.listening}
            Listening on {sync.addrs.length ? sync.addrs.join(" · ") : "this PC"}, port {sync.port}.
          {:else}
            Not listening yet.
          {/if}
        </p>
        {#if sync.peers.length > 0}
          <div class="peers">
            {#each sync.peers as peer (peer.deviceId)}
              <div class="peer">
                <span class="name">{peer.name}</span>
                <span class="muted">{peer.lastSeenAt ? `last synced ${when(peer.lastSeenAt, app.now)}` : "not synced yet"}</span>
                <button class="btn btn-quiet small" onclick={() => forget(peer.deviceId, peer.name)}>Forget</button>
              </div>
            {/each}
          </div>
        {/if}
        <div class="buttons">
          <button class="btn btn-primary" onclick={() => (pairingOpen = true)}>Pair a phone…</button>
        </div>
      {/if}
    </section>

    <section class="card">
      <h3>Backup</h3>
      <p class="hint muted">
        A backup is one JSON file with every reminder, timer, tag, preset and setting, plus this device's own
        settings.
      </p>
      <div class="buttons">
        <button class="btn" onclick={exportBackup}>Export…</button>
        <button class="btn" onclick={() => importBackup("merge")}>Merge a backup…</button>
        <button class="btn btn-danger" onclick={() => importBackup("replace")}>Replace from backup…</button>
      </div>
      <p class="hint muted">
        Merge keeps whatever is newer on either side. Replace makes this device — and, once they sync, your other
        devices — match the file.
      </p>
    </section>

    <Pairing bind:open={pairingOpen} status={sync} />
    {/if}

    <p class="about muted">Dun {version}</p>
  </div>
{/if}

<style>
  .settings {
    display: grid;
    gap: 0.8rem;
  }
  section {
    display: grid;
    gap: 0.5rem;
    padding: 0.8rem 0.9rem;
  }
  h3 {
    margin: 0 0 0.1rem;
    font-size: 0.95rem;
  }
  .scope {
    font-weight: 400;
    font-size: 0.75rem;
    color: var(--fg-faint);
    margin-left: 0.3rem;
  }
  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.6rem;
  }
  .narrow {
    width: 6rem;
  }
  .hint {
    margin: 0;
    font-size: 0.83rem;
  }
  .warn {
    color: var(--overdue);
  }
  .status {
    margin: 0;
    color: var(--overdue);
    font-weight: 600;
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
  .inline {
    display: flex;
    gap: 0.4rem;
    align-items: center;
  }
  .tags {
    display: grid;
    gap: 0.5rem;
  }
  .peers {
    display: grid;
    gap: 0.35rem;
  }
  .peer {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
    font-size: 0.9rem;
  }
  .peer .name {
    font-weight: 600;
  }
  .peer .muted {
    flex: 1;
    font-size: 0.83rem;
  }
  .small {
    min-height: 2rem;
    font-size: 0.85rem;
  }
  .tag {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .tag .input {
    flex: 1;
    min-width: 8rem;
  }
  .swatches {
    display: flex;
    gap: 0.2rem;
  }
  .swatch {
    width: 1.1rem;
    height: 1.1rem;
    border-radius: 999px;
    border: 2px solid transparent;
    cursor: pointer;
  }
  .swatch[aria-pressed="true"] {
    border-color: var(--fg);
  }
  input[type="range"] {
    width: 100%;
    accent-color: var(--accent);
  }
  .about {
    text-align: center;
    font-size: 0.8rem;
  }
</style>
