<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import AlarmSpike from "./views/AlarmSpike.svelte";

  let version = $state("");
  const isAndroid = navigator.userAgent.includes("Android");

  $effect(() => {
    invoke<string>("app_version")
      .then((v) => (version = v))
      .catch(() => (version = "browser preview"));
  });
</script>

<main>
  <h1>Dun</h1>
  <p class="muted">{version}</p>
  {#if isAndroid}
    <AlarmSpike />
  {/if}
</main>

<style>
  main {
    padding: max(2rem, env(safe-area-inset-top)) 1rem 2rem;
  }
  .muted {
    color: var(--fg-muted);
  }
</style>
