<script lang="ts" module>
  import type { ConnectionKind } from "../gen/ConnectionKind";
  import type { StatusTone } from "./StatusDot.svelte";

  export interface ContactRowProps {
    /** Primary label — nickname when present, otherwise a truncated y7 id. */
    label: string;
    /** Optional sublabel shown in the muted secondary line (e.g. truncated y7). */
    sublabel?: string;
    presence: StatusTone;
    /** Free-form tooltip — typically the full y7 id. */
    title: string;
    active?: boolean;
    /** V2-A4: when set, renders a small uppercase transport-kind label
     * (LAN / INTERNET / RELAY / DIRECT) next to the nickname. */
    connectionKind?: ConnectionKind;
    onclick?: (e: MouseEvent) => void;
  }
</script>

<script lang="ts">
  // Sidebar contact row. Two stacked lines (label + optional sublabel), a
  // leading presence dot, full-width clickable surface. Not coverable by
  // <Button> which centers a single child.

  import ConnectionLabel from "./ConnectionLabel.svelte";
  import StatusDot from "./StatusDot.svelte";

  let {
    label,
    sublabel,
    presence,
    title,
    active = false,
    connectionKind,
    onclick,
  }: ContactRowProps = $props();
</script>

<button
  type="button"
  class="row"
  data-active={active}
  {title}
  aria-label={label}
  onclick={(e) => onclick?.(e)}
>
  <StatusDot tone={presence} size={8} title={title} />
  <span class="meta">
    <span class="line">
      <span class="label">{label}</span>
      {#if connectionKind}
        <ConnectionLabel kind={connectionKind} />
      {/if}
    </span>
    {#if sublabel}
      <span class="sublabel">{sublabel}</span>
    {/if}
  </span>
</button>

<style>
  .row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: var(--y7-sp-3);
    padding: var(--y7-sp-2) var(--y7-sp-3);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--y7-r-md);
    color: var(--y7-text-secondary);
    font-family: var(--y7-font-mono);
    font-size: var(--y7-fs-md);
    text-align: left;
    cursor: pointer;
    transition:
      background-color var(--y7-dur-fast) var(--y7-ease),
      color var(--y7-dur-fast) var(--y7-ease),
      border-color var(--y7-dur-fast) var(--y7-ease);
    min-width: 0;
  }
  .row:hover {
    background: var(--y7-bg-hover);
    color: var(--y7-text-primary);
  }
  .row[data-active="true"] {
    background: var(--y7-bg-active);
    color: var(--y7-text-primary);
    border-color: var(--y7-border-default);
  }
  .meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 1px;
    flex: 1;
  }
  .line {
    display: flex;
    align-items: baseline;
    gap: var(--y7-sp-2);
    min-width: 0;
  }
  .label {
    font-size: var(--y7-fs-md);
    font-weight: var(--y7-fw-medium);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .sublabel {
    font-size: var(--y7-fs-xs);
    color: var(--y7-text-muted);
    letter-spacing: 0.02em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
