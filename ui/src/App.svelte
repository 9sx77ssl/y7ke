<script lang="ts">
  import { identity, loadIdentity } from "./lib/stores/identity.svelte";
  import { startEventDispatch } from "./lib/stores/events.svelte";
  import IdentitySetup from "./views/IdentitySetup.svelte";
  import MainShell from "./views/MainShell.svelte";
  import Titlebar from "./lib/components/Titlebar.svelte";
  import Toaster from "./lib/components/Toaster.svelte";

  // Boot: start event dispatch first (so identity_ready is captured even if
  // the get_my_id command resolves slightly later), then poll for the ID.
  $effect(() => {
    void startEventDispatch();
    void loadIdentity();
  });
</script>

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
