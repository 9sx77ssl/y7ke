<script lang="ts">
  // Two-pane shell: fixed-width sidebar on the left, flex center pane on the
  // right that swaps between empty / chat / add_contact / requests. Background
  // errors emitted from the Rust side surface via the global Toaster.

  import { router } from "../lib/stores/route.svelte";
  import {
    clearBackgroundError,
    eventState,
  } from "../lib/stores/events.svelte";
  import { toast } from "../lib/components/toast.svelte";
  import AddContact from "./AddContact.svelte";
  import Chat from "./Chat.svelte";
  import EmptyChat from "./EmptyChat.svelte";
  import Requests from "./Requests.svelte";
  import Settings from "./Settings.svelte";
  import Sidebar from "./Sidebar.svelte";

  // Forward backend background errors into the toast queue. We clear the
  // event-store slot immediately so re-emitting the same message string still
  // produces a fresh toast.
  $effect(() => {
    const msg = eventState.lastBackgroundError;
    if (msg !== null) {
      toast.error(msg);
      clearBackgroundError();
    }
  });
</script>

<div class="shell">
  <Sidebar />
  <section class="pane">
    {#if router.pane.kind === "empty"}
      <EmptyChat />
    {:else if router.pane.kind === "chat"}
      {#key router.pane.peerY7Id}
        <Chat peerY7Id={router.pane.peerY7Id} />
      {/key}
    {:else if router.pane.kind === "add_contact"}
      <AddContact />
    {:else if router.pane.kind === "requests"}
      <Requests />
    {:else if router.pane.kind === "settings"}
      <Settings />
    {/if}
  </section>
</div>

<style>
  .shell {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    background: var(--y7-bg-base);
  }
  .pane {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--y7-bg-base);
  }
</style>
