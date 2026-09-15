<script lang="ts">
  // M1 spike: exercises the Android alarm → receiver → Rust → notification loop.
  // Removed once the real Reminders UI drives Android alarms (M10).
  import { invoke } from "@tauri-apps/api/core";

  type Status = {
    device: { dataDir: string; tzId: string; sdkInt: number; manufacturer: string; model: string };
    setup: Record<string, unknown>;
    log: [number, string, string][];
  };

  let status = $state<Status | null>(null);
  let message = $state("");

  async function refresh() {
    try {
      status = await invoke<Status>("spike_status");
    } catch (e) {
      message = String(e);
    }
  }

  async function start(delaySeconds: number) {
    try {
      message = await invoke<string>("spike_start", { delaySeconds });
    } catch (e) {
      message = String(e);
    }
    await refresh();
  }

  function time(ms: number | unknown): string {
    return typeof ms === "number" && ms > 0 ? new Date(ms).toLocaleTimeString() : "—";
  }

  $effect(() => {
    invoke<string>("spike_app_started")
      .then((m) => (message = m))
      .catch((e) => (message = String(e)))
      .finally(refresh);
  });
</script>

<section>
  <h2>Alarm spike</h2>
  <div class="row">
    <button onclick={() => start(30)}>Ring in 30 s</button>
    <button onclick={() => start(120)}>Ring in 2 min</button>
    <button onclick={refresh}>Refresh</button>
  </div>
  {#if message}<p class="msg">{message}</p>{/if}

  {#if status}
    <dl>
      <dt>Device</dt>
      <dd>{status.device.manufacturer} {status.device.model} · SDK {status.device.sdkInt} · {status.device.tzId}</dd>
      <dt>Notifications</dt>
      <dd>{status.setup.notificationsEnabled ? "on" : "OFF"}</dd>
      <dt>Exact alarms</dt>
      <dd>{status.setup.exactAlarms ? "allowed" : "NOT allowed"}</dd>
      <dt>Battery</dt>
      <dd>{status.setup.ignoringBatteryOptimizations ? "unrestricted" : "optimized"}</dd>
      <dt>Next alarm</dt>
      <dd>{time(status.setup.scheduledAt)}</dd>
      <dt>Last fired</dt>
      <dd>{time(status.setup.lastFiredAt)} ({status.setup.lastLateByMs} ms late)</dd>
    </dl>
    <ol class="log">
      {#each status.log as [at, event, detail] (at + event)}
        <li><time>{time(at)}</time> {JSON.parse(event).type}: {detail}</li>
      {/each}
    </ol>
  {/if}
</section>

<style>
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  button {
    padding: 0.6rem 0.9rem;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--bg-raised);
    color: var(--fg);
    font: inherit;
  }
  .msg {
    color: var(--fg-muted);
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.2rem 0.8rem;
  }
  dt {
    color: var(--fg-muted);
  }
  dd {
    margin: 0;
  }
  .log {
    padding-left: 1.2rem;
    font-size: 0.85rem;
  }
  time {
    color: var(--fg-muted);
  }
</style>
