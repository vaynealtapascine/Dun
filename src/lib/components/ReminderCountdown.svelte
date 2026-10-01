<script lang="ts">
    import type { StatusView } from "../api/types";
    import {
        selectReminderCountdown,
        splitReminderCountdown,
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
    const parts = $derived(
        value
            ? [
                  { number: value.years, suffix: "y", separator: "" },
                  { number: value.months, suffix: "m", separator: " " },
                  { number: value.days, suffix: "d", separator: " " },
                  { number: value.hours, suffix: "", separator: " " },
                  { number: value.minutes, suffix: "", separator: ":" },
                  { number: value.seconds, suffix: "", separator: ":" },
              ]
            : [],
    );
    const firstSignificant = $derived(
        parts.findIndex((part) => part.number > 0),
    );
    const leadingEnd = $derived(firstSignificant < 0 ? 5 : firstSignificant);
</script>

{#if target && value}
    <div
        class="countdown"
        class:past={target.overdue}
        role="timer"
        aria-live="off"
        aria-label={`${title}: ${target.label} ${value.spoken}`}
    >
        <span aria-hidden="true"
            >{#each parts as part, index}<span
                    class:muted-zero={index < leadingEnd}
                    >{part.separator}{#if part.number > 0 && part.number < 10}<span
                            class="muted-zero">0</span
                        >{part.number}{:else}{String(part.number).padStart(
                            2,
                            "0",
                        )}{/if}{part.suffix}</span
                >{/each}</span
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
