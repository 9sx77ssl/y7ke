<script lang="ts" module>
  export interface NavItemProps {
    label: string;
    /** Optional leading glyph (string OR a snippet — string is simpler). */
    glyph?: string;
    /** Optional trailing number — renders as a pill badge if > 0. */
    badge?: number;
    /** When true, the row paints with the active background/border. */
    active?: boolean;
    /** Tooltip; required for accessibility (also used as aria-label). */
    title: string;
    onclick?: (e: MouseEvent) => void;
  }
</script>

<script lang="ts">
  // Sidebar nav row — a left-aligned, full-width button with optional glyph,
  // label, and trailing badge. NOT the same shape as Button (which centers
  // its content) and NOT the same as a contact row (which carries a status
  // dot + multi-line meta), so it lives as its own primitive.

  let {
    label,
    glyph,
    badge,
    active = false,
    title,
    onclick,
  }: NavItemProps = $props();
</script>

<button
  type="button"
  class="nav-item"
  data-active={active}
  {title}
  aria-label={title}
  onclick={(e) => onclick?.(e)}
>
  {#if glyph}
    <span class="glyph" aria-hidden="true">{glyph}</span>
  {/if}
  <span class="label">{label}</span>
  {#if typeof badge === "number" && badge > 0}
    <span class="badge" aria-label="{badge} pending">{badge}</span>
  {/if}
</button>

<style>
  .nav-item {
    display: flex;
    align-items: center;
    gap: var(--y7-sp-2);
    width: 100%;
    height: var(--y7-sz-btn-md);
    padding: 0 var(--y7-sp-3);
    border: 1px solid transparent;
    border-radius: var(--y7-r-md);
    background: transparent;
    color: var(--y7-text-secondary);
    font-family: var(--y7-font-mono);
    font-size: var(--y7-fs-md);
    text-align: left;
    text-transform: lowercase;
    letter-spacing: 0.02em;
    cursor: pointer;
    transition:
      background-color var(--y7-dur-fast) var(--y7-ease),
      color var(--y7-dur-fast) var(--y7-ease),
      border-color var(--y7-dur-fast) var(--y7-ease);
  }
  .nav-item:hover {
    background: var(--y7-bg-hover);
    color: var(--y7-text-primary);
  }
  .nav-item[data-active="true"] {
    background: var(--y7-bg-active);
    color: var(--y7-text-primary);
    border-color: var(--y7-border-default);
  }
  .glyph {
    font-weight: var(--y7-fw-bold);
    color: var(--y7-text-muted);
    width: var(--y7-sz-icon-md);
    text-align: center;
  }
  .nav-item:hover .glyph,
  .nav-item[data-active="true"] .glyph {
    color: var(--y7-text-primary);
  }
  .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    min-width: 18px;
    height: 18px;
    padding: 0 var(--y7-sp-2);
    border-radius: var(--y7-r-full);
    background: var(--y7-text-primary);
    color: var(--y7-text-on-accent);
    font-size: var(--y7-fs-xs);
    font-weight: var(--y7-fw-semibold);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
  }
</style>
