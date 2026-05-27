<script lang="ts">
  import { router } from "../lib/stores/route.svelte";
  import {
    clearBackgroundError,
    eventState,
  } from "../lib/stores/events.svelte";
  import AddContact from "./AddContact.svelte";
  import Chat from "./Chat.svelte";
  import EmptyChat from "./EmptyChat.svelte";
  import Requests from "./Requests.svelte";
  import Sidebar from "./Sidebar.svelte";
</script>

<div class="shell">
  <Sidebar />
  <main class="pane">
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
    {/if}
  </main>

  {#if eventState.lastBackgroundError}
    <div class="toast" role="status">
      <span>{eventState.lastBackgroundError}</span>
      <button
        type="button"
        onclick={clearBackgroundError}
        aria-label="Dismiss error"
      >
        ✕
      </button>
    </div>
  {/if}
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: auto 1fr;
    height: 100vh;
    min-height: 0;
    background: Canvas;
    color: CanvasText;
    font-family:
      ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  }
  .pane {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .toast {
    position: fixed;
    bottom: 1rem;
    right: 1rem;
    max-width: 28rem;
    padding: 0.6rem 0.85rem;
    border-radius: 8px;
    border: 1px solid color-mix(in oklab, currentColor 24%, transparent);
    background: color-mix(in oklab, Canvas 100%, crimson 12%);
    color: inherit;
    font-size: 0.85rem;
    display: flex;
    gap: 0.75rem;
    align-items: flex-start;
    box-shadow: 0 4px 18px color-mix(in oklab, Canvas 80%, transparent);
  }
  .toast button {
    font: inherit;
    background: transparent;
    border: none;
    color: inherit;
    cursor: pointer;
    padding: 0.1rem 0.35rem;
    border-radius: 4px;
    opacity: 0.65;
  }
  .toast button:hover {
    background: color-mix(in oklab, currentColor 12%, transparent);
    opacity: 1;
  }
</style>
