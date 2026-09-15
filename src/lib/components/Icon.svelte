<script lang="ts" module>
  // 24px stroke icons, drawn for Dun.
  const PATHS = {
    check: "M5 12.5l4.5 4.5L19 7.5",
    plus: "M12 5v14M5 12h14",
    gear: "M12 15.2a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4Zm7.4-2.2.1-1-.1-1 2-1.6-2-3.4-2.4.9a7.6 7.6 0 0 0-1.7-1L15 3h-4l-.4 2.6c-.6.3-1.2.6-1.7 1l-2.4-.9-2 3.4 2 1.6-.1 1 .1 1-2 1.6 2 3.4 2.4-.9c.5.4 1.1.7 1.7 1L11 21h4l.4-2.6c.6-.3 1.2-.6 1.7-1l2.4.9 2-3.4-2.1-1.6Z",
    snooze: "M4 8h5l-5 6h5M13 5h7l-7 9h7",
    repeat: "M17 2l3 3-3 3M4 11V9a4 4 0 0 1 4-4h12M7 22l-3-3 3-3M20 13v2a4 4 0 0 1-4 4H4",
    trash: "M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3",
    play: "M8 5.5v13l10.5-6.5L8 5.5Z",
    pause: "M8 5v14M16 5v14",
    reset: "M4 12a8 8 0 1 0 2.5-5.8M4 4v5h5",
    search: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14Zm5-2 4.5 4.5",
    close: "M6 6l12 12M18 6 6 18",
    bellOff: "M9 18a3 3 0 0 0 6 0M5.5 5.5 18.5 18.5M17 13.5V10a5 5 0 0 0-8.2-3.8M7 10v4l-2 2h10",
    bell: "M9 18a3 3 0 0 0 6 0M17 14V10a5 5 0 1 0-10 0v4l-2 2h14l-2-2Z",
    undo: "M9 14 4 9l5-5M4 9h11a5 5 0 0 1 0 10h-3",
    timer: "M12 21a8 8 0 1 0 0-16 8 8 0 0 0 0 16ZM12 9v4l2.5 2M10 2h4",
    list: "M9 6h11M9 12h11M9 18h11M4.5 6h.01M4.5 12h.01M4.5 18h.01",
    history: "M3.5 12A8.5 8.5 0 1 0 6 6M3 3v4h4M12 8v4l3 2",
    edit: "M4 20h4L19 9l-4-4L4 16v4ZM13.5 6.5l4 4",
    moon: "M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5Z",
  } as const;

  export type IconName = keyof typeof PATHS;
</script>

<script lang="ts">
  let { name, size = 20, label }: { name: IconName; size?: number; label?: string } = $props();
  const fill = $derived(name === "play" ? "currentColor" : "none");
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill={fill}
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  role={label ? "img" : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : "true"}
>
  <path d={PATHS[name]} />
</svg>
