<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let version = $state("");

  $effect(() => {
    invoke<string>("app_version")
      .then((v) => (version = v))
      .catch(() => (version = "browser preview"));
  });
</script>

<main>
  <h1>Dun</h1>
  <p class="muted">{version}</p>
</main>

<style>
  main {
    padding: 2rem 1rem;
  }
  .muted {
    color: var(--fg-muted);
  }
</style>
