<script lang="ts">
  import { identity, loadIdentity } from "./lib/stores/identity.svelte";
  import { startEventDispatch } from "./lib/stores/events.svelte";
  import IdentitySetup from "./views/IdentitySetup.svelte";
  import MainShell from "./views/MainShell.svelte";

  // Boot: start event dispatch first (so identity_ready is captured even if
  // the get_my_id command resolves slightly later), then poll for the ID.
  $effect(() => {
    void startEventDispatch();
    void loadIdentity();
  });
</script>

{#if !identity.isReady}
  <IdentitySetup />
{:else}
  <MainShell />
{/if}
