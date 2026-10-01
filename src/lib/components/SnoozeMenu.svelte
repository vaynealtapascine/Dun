<script lang="ts">
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  let { onsnooze }: { onsnooze: (minutes: number) => void } = $props();
  const OPTIONS: [number, string][] = [[1, "1 minute"], [5, "5 minutes"], [15, "15 minutes"], [30, "30 minutes"], [60, "1 hour"], [180, "3 hours"]];
  let open = $state(false);
</script>
<button class="btn" aria-haspopup="dialog" onclick={() => (open = true)}><Icon name="snooze" size={16} /> Snooze</button>
<Modal bind:open title="Snooze for">
  <div class="options">
    {#each OPTIONS as [minutes, label] (minutes)}
      <button class="btn" onclick={() => { open = false; onsnooze(minutes); }}>{label}</button>
    {/each}
  </div>
</Modal>
<style>
  .options { display: grid; grid-template-columns: repeat(2, 1fr); gap: 0.6rem; }
  .options .btn { min-height: 48px; justify-content: center; }
</style>
