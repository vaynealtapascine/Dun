<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { isPreview } from "../platform";
  import { app } from "../stores/app.svelte";
  let { notes }: { notes: string } = $props();
  const source = $derived(notes.match(/^From ([^\r\n]+)\r?\n(https?:\/\/[^\s]+)/));
  function open() {
    if (!source) return;
    if (isPreview) window.open(source[2], "_blank", "noopener,noreferrer");
    else app.run(() => invoke("open_source", { url: source[2] }));
  }
</script>

{#if source}
  <button class="source-link" type="button" onclick={open} title="Open original item in {source[1]}">Open in {source[1]} ↗</button>
{/if}

<style>
  .source-link { border: none; background: none; padding: 0.25rem 0; color: var(--accent); font-size: 0.8rem; cursor: pointer; text-align: left; }
  .source-link:hover { text-decoration: underline; }
</style>
