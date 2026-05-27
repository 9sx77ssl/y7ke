<script lang="ts" module>
  export interface ContextMenuItem {
    label: string;
    onClick: () => void;
    danger?: boolean;
    disabled?: boolean;
  }
</script>

<script lang="ts">
  interface Props {
    open: boolean;
    x: number;
    y: number;
    items: ContextMenuItem[];
    onClose?: () => void;
  }

  let { open = $bindable(false), x, y, items, onClose }: Props = $props();

  function close() {
    open = false;
    onClose?.();
  }

  function activate(item: ContextMenuItem) {
    if (item.disabled) return;
    close();
    item.onClick();
  }
</script>

<svelte:window
  onclick={() => open && close()}
  onkeydown={(e) => open && e.key === "Escape" && close()}
/>

{#if open}
  <ul
    class="menu"
    role="menu"
    style:left="{x}px"
    style:top="{y}px"
    onclick={(e) => e.stopPropagation()}
    oncontextmenu={(e) => e.preventDefault()}
  >
    {#each items as item}
      <li>
        <button
          type="button"
          role="menuitem"
          class="item"
          class:danger={item.danger}
          disabled={item.disabled}
          onclick={() => activate(item)}
        >
          {item.label}
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .menu {
    position: fixed;
    z-index: var(--y7-z-overlay);
    list-style: none;
    margin: 0;
    padding: var(--y7-sp-1);
    min-width: 160px;
    background: var(--y7-bg-elevated);
    border: 1px solid var(--y7-border-default);
    border-radius: var(--y7-r-md);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
    animation: pop var(--y7-dur-fast) var(--y7-ease);
  }
  .item {
    width: 100%;
    padding: var(--y7-sp-2) var(--y7-sp-3);
    background: transparent;
    border: none;
    border-radius: var(--y7-r-sm);
    color: var(--y7-text-primary);
    font-family: var(--y7-font-mono);
    font-size: var(--y7-fs-md);
    text-align: left;
    cursor: pointer;
    transition: background-color var(--y7-dur-fast) var(--y7-ease);
  }
  .item:hover:not(:disabled) {
    background: var(--y7-bg-hover);
  }
  .item:disabled {
    color: var(--y7-text-disabled);
    cursor: not-allowed;
  }
  .item.danger {
    color: var(--y7-red);
  }
  .item.danger:hover:not(:disabled) {
    background: var(--y7-red-soft);
  }
  @keyframes pop {
    from { opacity: 0; transform: scale(0.96); }
    to { opacity: 1; transform: scale(1); }
  }
</style>
