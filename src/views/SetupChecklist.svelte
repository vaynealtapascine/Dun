<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Icon from "../lib/components/Icon.svelte";

  /** What Android says about the permissions Dun needs to ring reliably. */
  type Status = {
    notifications?: boolean;
    exactAlarms?: boolean;
    batteryUnrestricted?: boolean;
    dndAccess?: boolean;
  };

  const CHECKS: { key: keyof Status; setting: string; title: string; why: string }[] = [
    {
      key: "notifications",
      setting: "notifications",
      title: "Show notifications",
      why: "Without this Dun can't tell you anything.",
    },
    {
      key: "exactAlarms",
      setting: "exactAlarms",
      title: "Alarms & reminders",
      why: "Lets Dun ring at the minute instead of whenever Android feels like it.",
    },
    {
      key: "dndAccess",
      setting: "dnd",
      title: "Ring through Do Not Disturb",
      why: "Timers go out on the alarm stream, which silent and Do Not Disturb normally allow. Grant this and they get through even if you've turned alarms off there too.",
    },
    {
      key: "batteryUnrestricted",
      setting: "battery",
      title: "Unrestricted battery use",
      why: "Samsung's battery saver otherwise stops Dun overnight — the most common cause of a missed reminder.",
    },
  ];

  let status = $state<Status>({});
  let checking = $state(true);

  $effect(() => {
    refresh();
    // Permissions usually change while the user is away in Settings.
    const onVisible = () => document.visibilityState === "visible" && refresh();
    document.addEventListener("visibilitychange", onVisible);
    return () => document.removeEventListener("visibilitychange", onVisible);
  });

  async function refresh() {
    try {
      status = await invoke<Status>("setup_status");
    } catch {
      status = {};
    } finally {
      checking = false;
    }
  }

  const outstanding = $derived(CHECKS.filter((c) => status[c.key] === false).length);
</script>

<section class="card" class:all-good={!checking && outstanding === 0}>
  <h3>
    Ringing reliably
    {#if !checking}
      <span class="count">{outstanding === 0 ? "all set" : `${outstanding} to fix`}</span>
    {/if}
  </h3>

  {#each CHECKS as check (check.key)}
    {@const ok = status[check.key] === true}
    <div class="check" class:ok>
      <span class="mark" aria-hidden="true">
        {#if ok}<Icon name="check" size={15} />{:else}<Icon name="bell" size={15} />{/if}
      </span>
      <div class="what">
        <span class="title">{check.title}</span>
        {#if !ok}<span class="why muted">{check.why}</span>{/if}
      </div>
      {#if !ok}
        <button class="btn" onclick={() => invoke("open_setting", { key: check.setting }).then(() => {})}>
          Open
        </button>
      {/if}
    </div>
  {/each}
</section>

<style>
  section {
    display: grid;
    gap: 0.6rem;
    padding: 0.8rem 0.9rem;
  }
  h3 {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    margin: 0;
    font-size: 0.95rem;
  }
  .count {
    font-size: 0.8rem;
    font-weight: 400;
    color: var(--fg-muted);
  }
  .all-good .count {
    color: var(--ok);
  }
  .check {
    display: flex;
    align-items: flex-start;
    gap: 0.6rem;
  }
  .mark {
    display: grid;
    place-items: center;
    width: 1.6rem;
    height: 1.6rem;
    flex: none;
    border-radius: 999px;
    background: var(--overdue-bg);
    color: var(--overdue);
  }
  .ok .mark {
    background: color-mix(in srgb, var(--ok) 18%, transparent);
    color: var(--ok);
  }
  .what {
    flex: 1;
    display: grid;
    gap: 0.1rem;
    min-width: 0;
  }
  .title {
    font-weight: 600;
  }
  .why {
    font-size: 0.83rem;
  }
</style>
