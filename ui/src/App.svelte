<script lang="ts">
  import { identity, loadIdentity } from "./lib/stores/identity.svelte";
  import { startEventDispatch } from "./lib/stores/events.svelte";
  import IdentitySetup from "./views/IdentitySetup.svelte";
  import MainShell from "./views/MainShell.svelte";
  import Titlebar from "./lib/components/Titlebar.svelte";
  import Toaster from "./lib/components/Toaster.svelte";

  // Suppress the native WebView right-click menu (Back / Forward / Reload).
  // Components opt in to our custom ContextMenu where appropriate.
  function suppressNativeContext(e: MouseEvent) {
    e.preventDefault();
  }

  $effect(() => {
    void startEventDispatch();
    void loadIdentity();
  });
</script>

<svelte:window oncontextmenu={suppressNativeContext} />

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
  }
</style>
