<script lang="ts">
  import { api, errorText } from "../lib/api/commands";
  import { app } from "../lib/stores/app.svelte";
  import Modal from "../lib/components/Modal.svelte";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let invite = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (!open) {
      invite = "";
      error = null;
    }
  });

  async function join() {
    if (!invite.trim()) return;
    busy = true;
    error = null;
    try {
      const name = await api.pairingJoin(invite.trim());
      app.notify(`Paired with ${name}`);
      open = false;
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal bind:open title="Join another PC">
  <ol class="steps">
    <li>On the other PC, open <strong>Settings → Sync → Pair a device</strong>.</li>
    <li>Under “Can't scan?”, copy the link and paste it here.</li>
  </ol>
  <label class="field">
    <span>Pairing link</span>
    <input
      class="input"
      bind:value={invite}
      placeholder="dun://pair?…"
      onkeydown={(e) => e.key === "Enter" && join()}
    />
  </label>
  <p class="muted note">Once paired, either PC can sync with the other, and everything paired with one reaches the other.</p>
  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#snippet footer()}
    <button class="btn" onclick={() => (open = false)}>Cancel</button>
    <button class="btn btn-primary" disabled={busy || !invite.trim()} onclick={join}>
      {busy ? "Pairing…" : "Pair"}
    </button>
  {/snippet}
</Modal>

<style>
  .steps {
    margin: 0;
    padding-left: 1.2rem;
    display: grid;
    gap: 0.2rem;
  }
  .note {
    margin: 0;
    font-size: 0.85rem;
  }
  .error {
    margin: 0;
    color: var(--ringing);
  }
</style>
