<script lang="ts" module>
  export type StatusTone = "online" | "offline" | "connecting" | "error";
</script>

<script lang="ts">
  interface Props {
    tone: StatusTone;
    size?: number;
    pulse?: boolean;
    title?: string;
  }

  let { tone, size = 8, pulse = false, title }: Props = $props();
</script>

<span
  class="dot tone-{tone}"
  class:pulse
  style:--y7-dot-size="{size}px"
  {title}
  aria-label={title ?? tone}
></span>

<style>
  .dot {
    display: inline-block;
    width: var(--y7-dot-size);
    height: var(--y7-dot-size);
    border-radius: var(--y7-r-full);
    flex-shrink: 0;
  }
  .tone-online {
    background: var(--y7-green);
    box-shadow: 0 0 4px var(--y7-green-soft);
  }
  .tone-offline {
    background: var(--y7-text-muted);
  }
  .tone-connecting {
    background: var(--y7-yellow);
  }
  .tone-error {
    background: var(--y7-red);
  }
  .pulse {
    animation: pulse 1.8s infinite;
  }
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }
</style>
