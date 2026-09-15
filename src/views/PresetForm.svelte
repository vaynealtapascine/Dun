<script lang="ts">
  import { untrack } from "svelte";
  import type { Preset } from "../lib/api/types";
  import { api } from "../lib/api/commands";
  import { joinDuration, splitDuration } from "../lib/dates";
  import { app } from "../lib/stores/app.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import Modal from "../lib/components/Modal.svelte";

  let { open = $bindable(false), preset = null }: { open?: boolean; preset?: Preset | null } = $props();

  let name = $state("");
  let h = $state(0);
  let m = $state(5);
  let s = $state(0);
  let nagOn = $state(true);
  let error = $state<string | null>(null);

  $effect(() => {
    if (open) untrack(reset);
  });

  function reset() {
    error = null;
    name = preset?.name ?? "";
    ({ h, m, s } = splitDuration(preset?.durationMs ?? 5 * 60_000));
    nagOn = (preset?.nag ?? app.snapshot?.settings.nagDefault)?.mode !== "once";
  }

  async function save() {
    const durationMs = joinDuration(h, m, s);
    if (!name.trim()) return (error = "Give the preset a name.");
    if (durationMs < 1000) return (error = "Set a duration.");
    try {
      await api.savePreset(preset?.id ?? null, {
        name: name.trim(),
        durationMs,
        nag: nagOn ? (app.snapshot?.settings.nagDefault ?? { mode: "repeat", intervalMin: 1 }) : { mode: "once" },
        chime: preset?.chime ?? null,
        tag: preset?.tag ?? null,
        order: preset?.order ?? app.snapshot?.presets.length ?? 0,
      });
      open = false;
    } catch (e) {
      error = String(e);
    }
  }
</script>

<Modal bind:open title={preset ? "Edit preset" : "New preset"}>
  <label class="field">
    <span>Name</span>
    <input class="input" bind:value={name} placeholder="Tea" onkeydown={(e) => e.key === "Enter" && save()} />
  </label>
  <div class="field">
    <span>Duration</span>
    <div class="duration">
      <label><input class="input" type="number" min="0" max="999" bind:value={h} /> h</label>
      <label><input class="input" type="number" min="0" max="59" bind:value={m} /> m</label>
      <label><input class="input" type="number" min="0" max="59" bind:value={s} /> s</label>
    </div>
  </div>
  <label class="switch"><span>Keep nagging until done</span><input type="checkbox" bind:checked={nagOn} /></label>
  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#snippet footer()}
    {#if preset}
      <button
        class="btn btn-danger btn-quiet"
        onclick={async () => {
          await app.run(() => api.deletePreset(preset!.id));
          open = false;
        }}><Icon name="trash" size={16} /> Delete</button
      >
      <span class="spacer"></span>
    {/if}
    <button class="btn" onclick={() => (open = false)}>Cancel</button>
    <button class="btn btn-primary" onclick={save}>Save</button>
  {/snippet}
</Modal>

<style>
  .duration {
    display: flex;
    gap: 0.5rem;
  }
  .duration label {
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }
  .duration .input {
    width: 4.5rem;
  }
  .error {
    margin: 0;
    color: var(--ringing);
  }
  .spacer {
    flex: 1;
  }
</style>
