<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { identity, loadIdentity } from "./lib/stores/identity.svelte";
  import { startEventDispatch } from "./lib/stores/events.svelte";
  import IdentitySetup from "./views/IdentitySetup.svelte";
  import MainShell from "./views/MainShell.svelte";
  import Titlebar from "./lib/components/Titlebar.svelte";
  import Toaster from "./lib/components/Toaster.svelte";

  type ResizeDir = 'East' | 'North' | 'NorthEast' | 'NorthWest' | 'South' | 'SouthEast' | 'SouthWest' | 'West';

  const win = getCurrentWindow();

  function suppressNativeContext(e: MouseEvent) {
    e.preventDefault();
  }

  function onResizeEdge(e: MouseEvent, dir: ResizeDir) {
    e.preventDefault();
    void win.startResizeDragging(dir);
  }

  $effect(() => {
    void startEventDispatch();
    void loadIdentity();
  });
</script>

<svelte:window oncontextmenu={suppressNativeContext} />

<!-- Resize handles: 4 edges + 4 corners. Must be outside .shell to sit at the window boundary. -->
<div class="rz rz-n"  role="presentation" onmousedown={(e) => onResizeEdge(e, 'North')}></div>
<div class="rz rz-s"  role="presentation" onmousedown={(e) => onResizeEdge(e, 'South')}></div>
<div class="rz rz-e"  role="presentation" onmousedown={(e) => onResizeEdge(e, 'East')}></div>
<div class="rz rz-w"  role="presentation" onmousedown={(e) => onResizeEdge(e, 'West')}></div>
<div class="rz rz-ne" role="presentation" onmousedown={(e) => onResizeEdge(e, 'NorthEast')}></div>
<div class="rz rz-nw" role="presentation" onmousedown={(e) => onResizeEdge(e, 'NorthWest')}></div>
<div class="rz rz-se" role="presentation" onmousedown={(e) => onResizeEdge(e, 'SouthEast')}></div>
<div class="rz rz-sw" role="presentation" onmousedown={(e) => onResizeEdge(e, 'SouthWest')}></div>

<div class="shell">
  <Titlebar />
  <main class="body">
    {#if !identity.isReady}
      <IdentitySetup />
    {:else}
      <MainShell />
    {/if}
  </main>
</div>
<Toaster />

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
  }
  .body {
    flex: 1;
    overflow: hidden;
    display: flex;
    min-height: 0;
    min-width: 0;
  }

  /* Resize handles — invisible, 6px wide, positioned at window edges.
   * Must sit BELOW the modal overlay (500) and toasts (1000): at 9999 the
   * edge strips intercepted mousedown near a dialog/scrollbar and started a
   * window resize instead. 90 keeps them grabbable at the bare window border
   * (titlebar/sidebar don't reach the outer 6px) without covering content. */
  .rz {
    position: fixed;
    z-index: 90;
  }
  .rz-n  { top: 0;    left: 6px;  right: 6px;  height: 6px; cursor: n-resize;  }
  .rz-s  { bottom: 0; left: 6px;  right: 6px;  height: 6px; cursor: s-resize;  }
  .rz-e  { top: 6px;  right: 0;   bottom: 6px; width: 6px;  cursor: e-resize;  }
  .rz-w  { top: 6px;  left: 0;    bottom: 6px; width: 6px;  cursor: w-resize;  }
  .rz-ne { top: 0;    right: 0;   width: 10px; height: 10px; cursor: ne-resize; }
  .rz-nw { top: 0;    left: 0;    width: 10px; height: 10px; cursor: nw-resize; }
  .rz-se { bottom: 0; right: 0;   width: 10px; height: 10px; cursor: se-resize; }
  .rz-sw { bottom: 0; left: 0;    width: 10px; height: 10px; cursor: sw-resize; }
</style>
