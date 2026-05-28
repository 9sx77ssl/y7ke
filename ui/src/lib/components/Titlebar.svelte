<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import IconButton from "./IconButton.svelte";
  import { copyDiagnostics } from "../diagnostics";
  import { toast } from "./toast.svelte";
  import { log } from "../log";

  const appWindow = getCurrentWindow();
  const logger = log("Titlebar");

  async function onMinimize() {
    await appWindow.minimize();
  }
  async function onToggleMaximize() {
    await appWindow.toggleMaximize();
  }
  async function onClose() {
    await appWindow.close();
  }

  // The little bug beside the logo: copy a full diagnostics snapshot
  // (transport/nat/dcutr/bootstraps + recent ui log) for a bug report.
  async function onCopyDiagnostics() {
    try {
      await copyDiagnostics();
      toast.success("diagnostics copied");
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      logger.error("copy diagnostics failed", msg);
      toast.error(`copy failed: ${msg}`);
    }
  }
</script>

<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <span class="dot" aria-hidden="true"></span>
    <span class="name">y7ke</span>
  </div>
  <button
    class="debug-btn"
    type="button"
    title="copy diagnostics to clipboard"
    aria-label="copy diagnostics"
    onclick={onCopyDiagnostics}
  >
    <svg
      width="14"
      height="14"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      stroke-width="1.1"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <!-- antennae -->
      <path d="M6.4 3 L5 1.5 M9.6 3 L11 1.5" />
      <!-- head -->
      <circle cx="8" cy="4.2" r="1.4" />
      <!-- body -->
      <ellipse cx="8" cy="9.4" rx="3.4" ry="4.3" />
      <!-- wing seam -->
      <path d="M8 5.4 L8 13.5" />
      <!-- left legs -->
      <path d="M4.7 7.1 L2.5 5.9 M4.4 9.4 L2.1 9.4 M4.7 11.7 L2.5 12.9" />
      <!-- right legs -->
      <path d="M11.3 7.1 L13.5 5.9 M11.6 9.4 L13.9 9.4 M11.3 11.7 L13.5 12.9" />
    </svg>
  </button>
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
  /* Subtle bug glyph beside the logo — muted at rest, brightens on hover.
   * Matches the line-art window controls; not in the drag region so it
   * stays clickable. */
  .debug-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    margin-left: var(--y7-sp-2);
    padding: 0;
    background: transparent;
    border: none;
    border-radius: var(--y7-r-sm);
    color: var(--y7-text-muted);
    cursor: pointer;
    transition:
      color var(--y7-dur-fast) var(--y7-ease),
      background-color var(--y7-dur-fast) var(--y7-ease);
  }
  .debug-btn:hover {
    color: var(--y7-text-secondary);
    background: var(--y7-bg-hover);
  }
  .debug-btn:focus-visible {
    outline: none;
    color: var(--y7-text-secondary);
    box-shadow: inset 0 0 0 1px var(--y7-border-focus);
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
