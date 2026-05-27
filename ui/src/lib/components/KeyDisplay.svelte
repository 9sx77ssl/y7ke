<script lang="ts" module>
  export type KeyDisplayLayout = "inline" | "block";
</script>

<script lang="ts">
  import { toast } from "./toast.svelte";

  interface Props {
    /** Full y7: URI. */
    value: string;
    /** Optional label shown above (block) or before (inline). */
    label?: string;
    /** "inline" = single row, label on the left; "block" = label on top, value in a code box. */
    layout?: KeyDisplayLayout;
    /** When true, only the truncated form is shown (with full value on hover via title attr). */
    truncate?: boolean;
  }

  let { value, label, layout = "inline", truncate = false }: Props = $props();

  function truncated(v: string): string {
    if (v.length <= 18) return v;
    return `${v.slice(0, 10)}…${v.slice(-6)}`;
  }

  let displayed = $derived(truncate ? truncated(value) : value);

  async function copy(): Promise<void> {
    try {
      await navigator.clipboard.writeText(value);
      toast.success("copied to clipboard");
    } catch (e) {
      toast.error(`copy failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  }
</script>

<div class="kd layout-{layout}">
  {#if label}
    <span class="label">{label}</span>
  {/if}
  <button
    type="button"
    class="value"
    title="click to copy — full: {value}"
    aria-label="copy {label ?? 'identity'}"
    onclick={copy}
  >
    <code>{displayed}</code>
    <span class="hint" aria-hidden="true">copy</span>
  </button>
</div>

<style>
  .kd {
    display: flex;
    gap: var(--y7-sp-2);
    min-width: 0;
  }
  .layout-inline {
    flex-direction: row;
    align-items: center;
  }
  .layout-block {
    flex-direction: column;
    align-items: stretch;
  }
  .label {
    font-size: var(--y7-fs-sm);
    color: var(--y7-text-secondary);
    text-transform: lowercase;
    letter-spacing: 0.02em;
    white-space: nowrap;
  }
  .value {
    display: flex;
    align-items: center;
    gap: var(--y7-sp-3);
    padding: var(--y7-sp-2) var(--y7-sp-3);
    background: var(--y7-bg-base);
    border: 1px solid var(--y7-border-default);
    border-radius: var(--y7-r-md);
    color: var(--y7-text-primary);
    font-family: var(--y7-font-mono);
    font-size: var(--y7-fs-md);
    line-height: 1;
    cursor: pointer;
    transition:
      background-color var(--y7-dur-fast) var(--y7-ease),
      border-color var(--y7-dur-fast) var(--y7-ease);
    min-width: 0;
  }
  .value:hover {
    background: var(--y7-bg-hover);
    border-color: var(--y7-border-strong);
  }
  .value:active {
    background: var(--y7-bg-active);
  }
  .value code {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    -webkit-user-select: none;
    user-select: none;
  }
  .hint {
    flex-shrink: 0;
    font-size: var(--y7-fs-xs);
    color: var(--y7-text-muted);
    text-transform: lowercase;
    letter-spacing: 0.04em;
    opacity: 0;
    transition: opacity var(--y7-dur-fast) var(--y7-ease);
  }
  .value:hover .hint {
    opacity: 1;
    color: var(--y7-green);
  }
  .layout-block .value {
    padding: var(--y7-sp-3);
  }
</style>
