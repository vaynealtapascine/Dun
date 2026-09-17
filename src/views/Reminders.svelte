<script lang="ts">
  import type { ItemView } from "../lib/api/types";
  import { api } from "../lib/api/commands";
  import { GROUP_TITLES, groupReminders } from "../lib/grouping";
  import { app } from "../lib/stores/app.svelte";
  import ItemRow from "../lib/components/ItemRow.svelte";
  import SwipeToDelete from "../lib/components/SwipeToDelete.svelte";

  let { items, onedit }: { items: ItemView[]; onedit: (item: ItemView) => void } = $props();

  const groups = $derived(groupReminders(items, app.now));
</script>

{#if groups.length === 0}
  <div class="empty">
    <p class="big">Nothing to nag you about.</p>
    <p class="muted">Add a reminder with the + button.</p>
  </div>
{:else}
  {#each groups as group (group.key)}
    <section>
      <h3 class:alert={group.key === "ringing"}>
        {GROUP_TITLES[group.key]} <span class="count">{group.items.length}</span>
      </h3>
      <div class="list" role="list">
        {#each group.items as item (item.id)}
          <SwipeToDelete role="listitem" label={item.title} ondelete={() => app.run(() => api.deleteItem(item.id))}>
            <ItemRow {item} {onedit} />
          </SwipeToDelete>
        {/each}
      </div>
    </section>
  {/each}
{/if}

<style>
  section {
    display: grid;
    gap: 0.4rem;
  }
  section + section {
    margin-top: 1rem;
  }
  h3 {
    margin: 0;
    padding: 0 0.2rem;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--fg-muted);
  }
  h3.alert {
    color: var(--ringing);
  }
  .count {
    font-weight: 400;
    color: var(--fg-faint);
  }
  .list {
    display: grid;
    gap: 0.4rem;
  }
  .empty {
    text-align: center;
    padding: 4rem 1rem;
  }
  .big {
    font-size: 1.1rem;
    font-weight: 600;
    margin-bottom: 0.2rem;
  }
</style>
