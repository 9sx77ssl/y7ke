<script lang="ts" module>
  export type ButtonVariant = "primary" | "ghost" | "danger" | "subtle";
  export type ButtonSize = "sm" | "md" | "lg";
</script>

<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    variant?: ButtonVariant;
    size?: ButtonSize;
    type?: "button" | "submit";
    disabled?: boolean;
    title?: string;
    ariaLabel?: string;
    fullWidth?: boolean;
    onclick?: (e: MouseEvent) => void;
    children?: Snippet;
  }

  let {
    variant = "primary",
    size = "md",
    type = "button",
    disabled = false,
    title,
    ariaLabel,
    fullWidth = false,
    onclick,
    children,
  }: Props = $props();
</script>

<button
  {type}
  {title}
  {disabled}
  aria-label={ariaLabel}
  class="btn variant-{variant} size-{size}"
  class:full={fullWidth}
  onclick={(e) => onclick?.(e)}
>
  {@render children?.()}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--y7-sp-2);
    border-radius: var(--y7-r-md);
    font-family: var(--y7-font-mono);
    font-weight: var(--y7-fw-medium);
    line-height: 1;
    white-space: nowrap;
    transition:
      background-color var(--y7-dur-fast) var(--y7-ease),
      border-color var(--y7-dur-fast) var(--y7-ease),
      color var(--y7-dur-fast) var(--y7-ease);
    border: 1px solid transparent;
  }

  .btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .full {
    width: 100%;
  }

  /* ---- sizes ---- */
  .size-sm {
    height: var(--y7-sz-btn-sm);
    padding: 0 var(--y7-sp-3);
    font-size: var(--y7-fs-sm);
  }
  .size-md {
    height: var(--y7-sz-btn-md);
    padding: 0 var(--y7-sp-4);
    font-size: var(--y7-fs-md);
  }
  .size-lg {
    height: var(--y7-sz-btn-lg);
    padding: 0 var(--y7-sp-5);
    font-size: var(--y7-fs-lg);
  }

  /* ---- variants ---- */
  .variant-primary {
    background: var(--y7-bg-active);
    border-color: var(--y7-border-strong);
    color: var(--y7-text-primary);
  }
  .variant-primary:hover:not(:disabled) {
    background: var(--y7-bg-hover);
    border-color: var(--y7-border-strong);
  }
  .variant-primary:active:not(:disabled) {
    background: var(--y7-bg-active);
  }

  .variant-ghost {
    background: transparent;
    border-color: var(--y7-border-default);
    color: var(--y7-text-primary);
  }
  .variant-ghost:hover:not(:disabled) {
    background: var(--y7-bg-hover);
    border-color: var(--y7-border-strong);
  }

  .variant-subtle {
    background: transparent;
    border-color: transparent;
    color: var(--y7-text-secondary);
  }
  .variant-subtle:hover:not(:disabled) {
    background: var(--y7-bg-hover);
    color: var(--y7-text-primary);
  }

  .variant-danger {
    background: transparent;
    border-color: var(--y7-red-dim);
    color: var(--y7-red);
  }
  .variant-danger:hover:not(:disabled) {
    background: var(--y7-red-soft);
  }
</style>
