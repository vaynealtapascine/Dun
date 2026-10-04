<script lang="ts">
    import type { StatusView } from "../api/types";
    import {
        selectReminderCountdown,
        splitReminderCountdown,
        splitReminderReadout,
    } from "../reminderCountdown";

    let {
        title,
        status,
        now,
    }: { title: string; status: StatusView; now: number } = $props();
    const target = $derived(selectReminderCountdown(status, now));
    const value = $derived(
        target ? splitReminderCountdown(now, target.at) : null,
    );
    const readout = $derived(value ? splitReminderReadout(value) : null);
</script>

{#if target && value && readout}
    <div
        class="countdown"
        class:past={target.overdue}
        role="timer"
        aria-live="off"
        aria-label={`${title}: ${target.label} ${value.spoken}`}
    >
        <span aria-hidden="true"
            ><span class="muted-zero">{readout.leading}</span
            >{readout.rest}</span
        >
    </div>
{/if}

<style>
    .countdown {
        color: var(--fg);
        font-family: var(--font-timer);
        font-variant-numeric: lining-nums tabular-nums;
        font-size: 1.5rem;
        font-weight: 600;
        line-height: 1.2;
        white-space: nowrap;
        text-align: right;
    }
    .past {
        color: var(--ringing);
    }
    .muted-zero {
        opacity: 0.42;
    }
    @media (max-width: 600px), (min-width: 760px) and (max-aspect-ratio: 3/4) {
        .countdown {
            font-size: 1.25rem;
        }
    }
</style>
