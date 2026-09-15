<script lang="ts">
  import { untrack } from "svelte";
  import type { PairingView, SyncStatus } from "../lib/api/types";
  import { api } from "../lib/api/commands";
  import { app } from "../lib/stores/app.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import Modal from "../lib/components/Modal.svelte";

  let { open = $bindable(false), status }: { open?: boolean; status: SyncStatus | null } = $props();

  let pairing = $state<PairingView | null>(null);
  let error = $state<string | null>(null);
  let copied = $state(false);

  // Whoever pairs shows up in `status.peers`; one more than when this dialog
  // opened means the phone got through. Must be state, or the count it's
  // compared against never updates.
  const pairedCount = $derived(status?.peers.length ?? 0);
  let countAtOpen = $state<number | null>(null);
  const justPaired = $derived(countAtOpen !== null && pairedCount > countAtOpen);

  const secondsLeft = $derived(pairing ? Math.max(0, Math.round((pairing.expiresAt - app.now) / 1000)) : 0);
  const expired = $derived(!!pairing && secondsLeft === 0);

  $effect(() => {
    if (open) {
      untrack(start);
    } else {
      pairing = null;
      countAtOpen = null;
    }
  });

  async function start() {
    error = null;
    copied = false;
    countAtOpen = status?.peers.length ?? 0;
    pairing = (await app.run(() => api.pairingOpen())) ?? null;
    if (!pairing) error = app.error;
  }

  function close() {
    api.pairingClose().catch(() => {});
    open = false;
  }

  async function copyLink() {
    if (!pairing) return;
    await navigator.clipboard.writeText(pairing.uri);
    copied = true;
    setTimeout(() => (copied = false), 2000);
  }
</script>

<Modal bind:open title="Pair your phone">
  {#if justPaired}
    <div class="done">
      <p class="big">Paired with {status?.peers.at(-1)?.name ?? "your phone"}.</p>
      <p class="muted">Reminders, timers and settings will keep in step whenever both are on the same network.</p>
    </div>
  {:else if pairing}
    <ol class="steps">
      <li>Open Dun on your phone and tap <strong>Pair with a PC</strong>.</li>
      <li>Scan this code, or type the six digits.</li>
    </ol>

    <div class="qr" class:expired>
      <!-- Generated locally by Dun, not user content. -->
      {@html pairing.qrSvg}
    </div>

    <div class="code" class:expired>{pairing.code.slice(0, 3)} {pairing.code.slice(3)}</div>

    <p class="muted timing">
      {#if expired}
        This code has expired.
      {:else}
        Expires in {Math.floor(secondsLeft / 60)}:{String(secondsLeft % 60).padStart(2, "0")} · {pairing.triesLeft} tries left
      {/if}
    </p>

    {#if status && status.addrs.length > 0}
      <p class="muted addrs">This PC: {status.addrs.join(" · ")}</p>
    {/if}

    <details>
      <summary>Can't scan?</summary>
      <p class="muted">Paste this into the phone's pairing screen:</p>
      <div class="link">
        <code>{pairing.uri}</code>
        <button class="btn" onclick={copyLink}>{copied ? "Copied" : "Copy"}</button>
      </div>
    </details>
  {:else if error}
    <p class="error" role="alert">{error}</p>
  {:else}
    <p class="muted">Getting a code…</p>
  {/if}

  {#snippet footer()}
    {#if justPaired}
      <button class="btn btn-primary" onclick={close}>Done</button>
    {:else}
      <button class="btn" onclick={close}>Cancel</button>
      {#if expired}
        <button class="btn btn-primary" onclick={start}><Icon name="reset" size={15} /> New code</button>
      {/if}
    {/if}
  {/snippet}
</Modal>

<style>
  .steps {
    margin: 0;
    padding-left: 1.2rem;
    display: grid;
    gap: 0.2rem;
  }
  .qr {
    display: grid;
    place-items: center;
    padding: 0.6rem;
    background: #fff;
    border-radius: var(--radius);
    border: 1px solid var(--border);
  }
  .qr :global(svg) {
    width: min(240px, 60vw);
    height: auto;
    display: block;
  }
  .expired {
    opacity: 0.35;
  }
  .code {
    text-align: center;
    font-size: 2rem;
    font-weight: 700;
    letter-spacing: 0.12em;
    font-variant-numeric: tabular-nums;
  }
  .timing,
  .addrs {
    margin: 0;
    text-align: center;
    font-size: 0.85rem;
  }
  details summary {
    cursor: pointer;
    font-size: 0.9rem;
  }
  .link {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    margin-top: 0.3rem;
  }
  .link code {
    flex: 1;
    min-width: 0;
    padding: 0.4rem 0.6rem;
    border-radius: var(--radius-sm);
    background: var(--bg-sunken);
    font-size: 0.75rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .done {
    text-align: center;
    padding: 1rem 0;
  }
  .big {
    font-size: 1.1rem;
    font-weight: 600;
    margin: 0 0 0.3rem;
  }
  .error {
    color: var(--ringing);
  }
</style>
