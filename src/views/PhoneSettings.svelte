<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { ChimeRef, Ms } from "../lib/api/types";
  import { when } from "../lib/format";
  import { app } from "../lib/stores/app.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import SetupChecklist from "./SetupChecklist.svelte";

  type PhoneSync = {
    paired: boolean;
    pcName: string | null;
    lastSeenAt: Ms | null;
    addrs: string[];
    chime: ChimeRef;
    /** Set when this phone's clock and the PC's disagree enough to matter. */
    skewMs: number | null;
  };

  // Android ties a sound to a notification channel, so only the bundled ones.
  const CHIMES: { id: string; label: string }[] = [
    { id: "bell", label: "Bell" },
    { id: "rise", label: "Rise" },
    { id: "pulse", label: "Pulse" },
    { id: "soft", label: "Soft" },
  ];

  let sync = $state<PhoneSync | null>(null);
  let invite = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    refresh();
  });

  async function refresh() {
    try {
      sync = await invoke<PhoneSync>("sync_status");
    } catch (e) {
      error = String(e);
    }
  }

  async function pair() {
    if (!invite.trim()) return;
    busy = true;
    error = null;
    try {
      const pcName = await invoke<string>("pair_with_pc", {
        invite: invite.trim(),
        name: deviceName(),
      });
      invite = "";
      app.notify(`Paired with ${pcName}`);
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function syncNow() {
    busy = true;
    error = null;
    try {
      sync = await invoke<PhoneSync>("sync_now");
      app.notify("Synced");
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function forget() {
    if (!confirm(`Forget ${sync?.pcName ?? "this PC"}? Your reminders stay on this phone.`)) return;
    await app.run(() => invoke("forget_pc"));
    await refresh();
  }

  function setChime(id: string) {
    const chime: ChimeRef = { kind: "bundled", id };
    if (sync) sync = { ...sync, chime };
    app.run(() => invoke("set_chime", { chime }));
  }

  /** Something recognisable in the PC's device list. */
  function deviceName(): string {
    const match = navigator.userAgent.match(/Android[^;]*;\s*([^)]+)\)/);
    return match?.[1]?.split(" Build")[0]?.trim() || "Phone";
  }
</script>

<section class="card">
  <h3>This phone</h3>
  <div class="field">
    <span>Chime</span>
    <div class="chimes">
      {#each CHIMES as c (c.id)}
        <button
          class="chip"
          aria-pressed={sync?.chime.kind === "bundled" && sync.chime.id === c.id}
          onclick={() => setChime(c.id)}>{c.label}</button
        >
      {/each}
    </div>
    <p class="hint muted">Each chime is its own notification channel, so Android's own volume and Do Not Disturb rules apply per chime.</p>
  </div>
</section>

<section class="card">
  <h3>Sync with your PC</h3>
  {#if sync?.paired}
    <p class="status">
      Paired with <strong>{sync.pcName}</strong>
      {#if sync.lastSeenAt}· synced {when(sync.lastSeenAt, app.now)}{/if}
    </p>
    {#if sync.addrs.length}
      <p class="hint muted">Tries: {sync.addrs.join(" · ")}</p>
    {/if}
    {#if sync.skewMs != null}
      <p class="hint warn">
        This phone's clock is {Math.abs(Math.round(sync.skewMs / 1000))} s {sync.skewMs > 0 ? "ahead of" : "behind"}
        {sync.pcName ?? "the PC"}, so reminders can ring at the wrong moment. Set both to update the time
        automatically.
      </p>
    {/if}
    <div class="buttons">
      <button class="btn" disabled={busy} onclick={syncNow}><Icon name="repeat" size={15} /> Sync now</button>
      <button class="btn btn-danger btn-quiet" onclick={forget}>Forget PC</button>
    </div>
  {:else}
    <p class="hint muted">
      On the PC open <strong>Settings → Sync → Pair a phone</strong>, then paste the link shown under “Can't scan?”.
    </p>
    <label class="field">
      <span>Pairing link</span>
      <input class="input" bind:value={invite} placeholder="dun://pair?…" autocapitalize="off" autocorrect="off" />
    </label>
    <div class="buttons">
      <button class="btn btn-primary" disabled={busy || !invite.trim()} onclick={pair}>Pair</button>
    </div>
  {/if}
  {#if error}<p class="hint warn">{error}</p>{/if}
</section>

<SetupChecklist />

<style>
  section {
    display: grid;
    gap: 0.5rem;
    padding: 0.8rem 0.9rem;
  }
  h3 {
    margin: 0 0 0.1rem;
    font-size: 0.95rem;
  }
  .chimes {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }
  .chip {
    padding: 0.35rem 0.8rem;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--bg-raised);
    cursor: pointer;
  }
  .chip[aria-pressed="true"] {
    border-color: var(--accent);
    color: var(--accent);
    font-weight: 600;
  }
  .status {
    margin: 0;
  }
  .hint {
    margin: 0;
    font-size: 0.83rem;
  }
  .warn {
    color: var(--ringing);
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
</style>
