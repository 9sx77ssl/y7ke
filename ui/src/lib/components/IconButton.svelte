<script lang="ts" module>
  export type IconButtonTone = "default" | "danger" | "accent";
</script>

<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    tone?: IconButtonTone;
    size?: number;
    title?: string;
    ariaLabel: string;
    disabled?: boolean;
    onclick?: (e: MouseEvent) => void;
    children?: Snippet;
  }

  let {
    tone = "default",
    size = 28,
    title,
    ariaLabel,
    disabled = false,
    onclick,
    children,
  }: Props = $props();
</script>

<button
  type="button"
  {title}
  aria-label={ariaLabel}
  {disabled}
  class="icon-btn tone-{tone}"
  style:--y7-icon-size="{size}px"
  onclick={(e) => onclick?.(e)}
>
  {@render children?.()}
</button>

<style>
  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--y7-icon-size);
    height: var(--y7-icon-size);
    border-radius: var(--y7-r-md);
    color: var(--y7-text-secondary);
    transition:
      background-color var(--y7-dur-fast) var(--y7-ease),
      color var(--y7-dur-fast) var(--y7-ease);
  }
  .icon-btn:hover:not(:disabled) {
    background: var(--y7-bg-hover);
    color: var(--y7-text-primary);
  }
  .icon-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  .tone-danger:hover:not(:disabled) {
    /* Neutral, matches every other IconButton. */
    background: var(--y7-bg-hover);
    color: var(--y7-text-primary);
  }
  .tone-accent:hover:not(:disabled) {
    background: var(--y7-green-soft);
    color: var(--y7-green);
  }
</style>
