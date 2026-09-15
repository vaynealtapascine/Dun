<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";

  let {
    open = $bindable(false),
    title,
    children,
    footer,
  }: { open?: boolean; title: string; children: Snippet; footer?: Snippet } = $props();

  let dialog: HTMLDialogElement | undefined = $state();

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  });
</script>

<dialog
  bind:this={dialog}
  onclose={() => (open = false)}
  onclick={(e) => {
    // Click on the backdrop closes.
    if (e.target === dialog) open = false;
  }}
>
  {#if open}
    <div class="sheet">
      <header>
        <h2>{title}</h2>
        <button class="icon-btn" onclick={() => (open = false)} aria-label="Close">
          <Icon name="close" />
        </button>
      </header>
      <div class="body">{@render children()}</div>
      {#if footer}
        <footer>{@render footer()}</footer>
      {/if}
    </div>
  {/if}
</dialog>

<style>
  dialog {
    padding: 0;
    border: none;
    background: transparent;
    width: min(34rem, calc(100vw - 1.5rem));
    max-height: calc(100vh - 1.5rem);
    color: var(--fg);
  }
  dialog::backdrop {
    background: rgba(0, 0, 0, 0.35);
  }
  .sheet {
    display: flex;
    flex-direction: column;
    max-height: calc(100vh - 1.5rem);
    background: var(--bg);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    border: 1px solid var(--border);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem 0.75rem 0.25rem 1.1rem;
  }
  h2 {
    margin: 0;
    font-size: 1.1rem;
  }
  .body {
    padding: 0.5rem 1.1rem 1rem;
    overflow-y: auto;
    display: grid;
    gap: 0.9rem;
  }
  footer {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
    padding: 0.75rem 1.1rem;
    border-top: 1px solid var(--border);
  }
</style>
