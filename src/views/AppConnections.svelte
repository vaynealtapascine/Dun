<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { app } from "../lib/stores/app.svelte";
  type Connection = { enabled: boolean; running: boolean; url: string; token: string; error: string | null };
  let connection = $state<Connection | null>(null);
  let error = $state("");
  let busy = $state(false);
  let showKey = $state(false);

  $effect(() => { invoke<Connection>("integration_status").then(s => connection = s).catch(e => error = String(e)); });
  async function change(command: string, args?: Record<string, unknown>) {
    busy = true; error = "";
    try { connection = await invoke<Connection>(command, args); }
    catch (e) { error = String(e); }
    finally { busy = false; }
  }
  async function copy(value: string, label: string) {
    try { await navigator.clipboard.writeText(value); app.notify(`${label} copied`); }
    catch { error = "Couldn't copy. Select the value below and copy it manually."; showKey = true; }
  }
</script>

<section class="card connections">
  <h3>App connections <span class="scope">this PC</span></h3>
  <p class="hint muted">Let Memos, Arbor, and other apps add reminders and start timers. Items sync to your paired phone like anything you add in Dun.</p>
  {#if connection}
    <label class="switch">
      <span>Allow apps to send to Dun</span>
      <input type="checkbox" disabled={busy} checked={connection.enabled} onchange={e => change("integration_set_enabled", { enabled: e.currentTarget.checked })} />
    </label>
    {#if connection.enabled}
      <p class="hint">{connection.running ? "Ready for connections on this PC" : "Connection unavailable"}</p>
      <label class="field"><span>Connection address</span><input class="input" readonly value={connection.url} /></label>
      <div class="buttons">
        <button class="btn" onclick={() => copy(connection!.url, "Address")}>Copy address</button>
        <button class="btn" onclick={() => copy(connection!.token, "Connection key")}>Copy key</button>
        <button class="btn" onclick={() => showKey = !showKey}>{showKey ? "Hide key" : "Show key"}</button>
      </div>
      {#if showKey}<label class="field"><span>Private connection key</span><input class="input" readonly value={connection.token} /></label>{/if}
      <p class="hint muted">Use the address and key in the sending app's server settings. Keep Dun running in the tray. The connection stays on this PC; it does not need phone pairing credentials.</p>
      <details><summary>Replace a connection key</summary>
        <p class="hint muted">Existing apps will need the new key before they can send again.</p>
        <button class="btn" disabled={busy} onclick={() => change("integration_rotate_token")}>Generate new key</button>
      </details>
    {/if}
    {#if connection.error}<p class="hint warn" role="alert">{connection.error}</p>{/if}
  {/if}
  {#if error}<p class="hint warn" role="alert">{error}</p>{/if}
</section>

<style>
  .connections { display: grid; gap: 0.75rem; padding: 0.8rem 1rem; }
  h3 { font-size: 0.9rem; }
  h3, p { margin: 0; }
  .scope { font-size: 0.72rem; font-weight: 400; color: var(--fg-muted); }
  .buttons { display: flex; flex-wrap: wrap; gap: 0.5rem; }
  summary { cursor: pointer; font-size: 0.85rem; }
</style>
