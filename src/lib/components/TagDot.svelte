<script lang="ts">
  let { color, size = 8 }: { color: string; size?: number } = $props();
</script>

<span class="dot" style:background={color} style:width="{size}px" style:height="{size}px" aria-hidden="true"></span>

<style>
  .dot {
    display: inline-block;
    flex: none;
    border-radius: 999px;
  }
</style>
