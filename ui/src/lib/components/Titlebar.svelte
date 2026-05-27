<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import IconButton from "./IconButton.svelte";

  const appWindow = getCurrentWindow();

  async function onMinimize() {
    await appWindow.minimize();
  }
  async function onToggleMaximize() {
    await appWindow.toggleMaximize();
  }
  async function onClose() {
    await appWindow.close();
  }
</script>

<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <span class="dot" aria-hidden="true"></span>
    <span class="name">y7ke</span>
  </div>
  <div class="spacer" data-tauri-drag-region></div>
  <div class="controls" aria-label="window controls">
    <IconButton
      size={28}
      ariaLabel="minimize"
      title="minimize"
      onclick={onMinimize}
    >
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
        <line
          x1="1"
          y1="5"
          x2="9"
          y2="5"
          stroke="currentColor"
          stroke-width="1.2"
          stroke-linecap="round"
        />
      </svg>
    </IconButton>
    <IconButton
      size={28}
      ariaLabel="maximize"
      title="maximize"
      onclick={onToggleMaximize}
    >
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
        <rect
          x="1.5"
          y="1.5"
          width="7"
          height="7"
          fill="none"
          stroke="currentColor"
          stroke-width="1.2"
          rx="1"
        />
      </svg>
    </IconButton>
    <IconButton
      tone="danger"
      size={28}
      ariaLabel="close"
      title="close"
      onclick={onClose}
    >
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
        <line
          x1="2"
          y1="2"
          x2="8"
          y2="8"
          stroke="currentColor"
          stroke-width="1.2"
          stroke-linecap="round"
        />
        <line
          x1="8"
          y1="2"
          x2="2"
          y2="8"
          stroke="currentColor"
          stroke-width="1.2"
          stroke-linecap="round"
        />
      </svg>
    </IconButton>
  </div>
</header>

<style>
  .titlebar {
    height: var(--y7-sz-titlebar);
    display: flex;
    align-items: center;
    padding: 0 var(--y7-sp-3);
    background: var(--y7-bg-sidebar);
    border-bottom: 1px solid var(--y7-border-subtle);
    z-index: var(--y7-z-titlebar);
    flex-shrink: 0;
    -webkit-user-select: none;
    user-select: none;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: var(--y7-sp-2);
    font-size: var(--y7-fs-md);
    font-weight: var(--y7-fw-bold);
    color: var(--y7-text-primary);
    letter-spacing: 0.02em;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--y7-green);
  }
  .name {
    text-transform: lowercase;
  }
  .spacer {
    flex: 1;
  }
  .controls {
    display: flex;
    align-items: center;
    gap: var(--y7-sp-1);
  }
</style>
