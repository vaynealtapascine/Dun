<script lang="ts">
  import Icon from "./Icon.svelte";

  let { onsnooze }: { onsnooze: (minutes: number) => void } = $props();

  const OPTIONS: [number, string][] = [
    [1, "1 min"],
    [5, "5 min"],
    [15, "15 min"],
    [30, "30 min"],
    [60, "1 hour"],
    [180, "3 hours"],
  ];

  let open = $state(false);
  let root: HTMLDivElement | undefined = $state();

  $effect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (root && !root.contains(e.target as Node)) open = false;
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && (open = false);
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", esc);
    };
  });
</script>

<div class="wrap" bind:this={root}>
  <button
    class="btn"
    aria-haspopup="menu"
    aria-expanded={open}
    onclick={(e) => {
      e.stopPropagation();
      open = !open;
    }}
  >
    <Icon name="snooze" size={16} /> Snooze
  </button>
  {#if open}
    <div class="menu card" role="menu">
      {#each OPTIONS as [minutes, label] (minutes)}
        <button
          role="menuitem"
          onclick={(e) => {
            e.stopPropagation();
            open = false;
            onsnooze(minutes);
          }}>{label}</button
        >
      {/each}
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
  }
  .menu {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    z-index: 10;
    display: grid;
    min-width: 8rem;
    padding: 0.3rem;
    box-shadow: var(--shadow);
  }
  .menu button {
    text-align: left;
    padding: 0.45rem 0.7rem;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    cursor: pointer;
  }
  .menu button:hover {
    background: var(--bg-sunken);
  }
</style>
