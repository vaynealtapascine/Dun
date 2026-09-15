<script lang="ts">
  // M2 spike: Windows toast buttons via the COM activator. Removed in M6.
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";

  type Status = { aumid: string; blocked: string | null; logPath: string; log: string[] };

  let status = $state<Status | null>(null);
  let message = $state("");

  async function refresh() {
    try {
      status = await invoke<Status>("toast_spike_status");
    } catch (e) {
      message = String(e);
    }
  }

  async function run(cmd: string, args: Record<string, unknown> = {}) {
    try {
      message = await invoke<string>(cmd, args);
    } catch (e) {
      message = String(e);
    }
    await refresh();
  }

  function time(line: string): string {
    const [ms, ...rest] = line.split("\t");
    return `${new Date(Number(ms)).toLocaleTimeString()}  ${rest.join(" ")}`;
  }

  $effect(() => {
    refresh();
    const unlisten = listen<string>("toast-spike", () => refresh());
    return () => {
      unlisten.then((f) => f());
    };
  });
</script>

<section>
  <h2>Toast spike</h2>
  <div class="row">
    <button onclick={() => run("toast_spike_show", { kind: "ring" })}>Show toast</button>
    <button onclick={() => run("toast_spike_show", { kind: "replace" })}>Show again (same tag)</button>
    <button onclick={() => run("toast_spike_remove")}>Remove</button>
    <button onclick={refresh}>Refresh</button>
  </div>
  {#if message}<p class="msg">{message}</p>{/if}
  {#if status}
    <p class="msg">
      {status.aumid} · {status.blocked ?? "notifications enabled"} · {status.logPath}
    </p>
    <ol class="log">
      {#each status.log as line, i (i)}
        <li>{time(line)}</li>
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
    padding: 0.5rem 0.8rem;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--bg-raised);
    color: var(--fg);
    font: inherit;
  }
  .msg {
    color: var(--fg-muted);
    font-size: 0.85rem;
    overflow-wrap: anywhere;
  }
  .log {
    padding-left: 1.2rem;
    font-size: 0.8rem;
    overflow-wrap: anywhere;
  }
</style>
