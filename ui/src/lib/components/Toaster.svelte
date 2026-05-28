<script lang="ts">
  import { toast } from "./toast.svelte";
</script>

<div class="toaster" aria-live="polite" aria-atomic="true">
  {#each toast.queue as t (t.id)}
    <div class="toast tone-{t.tone}">
      <span class="msg">{t.message}</span>
    </div>
  {/each}
</div>

<style>
  .toaster {
    position: fixed;
    left: 0;
    /* Leave room for the sidebar footer ("N contacts" + safe gap). */
    bottom: calc(var(--y7-sp-8) + var(--y7-sp-2));
    width: var(--y7-sz-sidebar);
    padding: var(--y7-sp-2);
    display: flex;
    flex-direction: column-reverse;
    gap: var(--y7-sp-2);
    z-index: var(--y7-z-toast);
    pointer-events: none;
    /* Cap the stack so wrapped long messages can't tower over the
     * contacts list; older toasts scroll out of the clipped top. */
    max-height: 40vh;
    overflow: hidden;
  }
  .toast {
    pointer-events: auto;
    width: 100%;
    padding: var(--y7-sp-2) var(--y7-sp-4);
    background: var(--y7-bg-elevated);
    border: 1px solid var(--y7-border-default);
    border-radius: var(--y7-r-md);
    color: var(--y7-text-primary);
    font-size: var(--y7-fs-sm);
    line-height: var(--y7-lh-normal);
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.4);
    animation: slide-in var(--y7-dur-base) var(--y7-ease);
  }
  .tone-success {
    border-color: var(--y7-green-dim);
    color: var(--y7-green);
  }
  .tone-error {
    border-color: var(--y7-red-dim);
    color: var(--y7-red);
  }
  .msg {
    text-transform: lowercase;
    letter-spacing: 0.02em;
  }
  @keyframes slide-in {
    from {
      transform: translateY(8px);
      opacity: 0;
    }
    to {
      transform: translateY(0);
      opacity: 1;
    }
  }
</style>
