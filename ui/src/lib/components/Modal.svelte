<script lang="ts" module>
  export type ModalTone = "default" | "danger";
</script>

<script lang="ts">
  import type { Snippet } from "svelte";
  import Button from "./Button.svelte";

  interface Props {
    open: boolean;
    title: string;
    description?: string;
    confirmLabel?: string;
    cancelLabel?: string;
    tone?: ModalTone;
    onConfirm?: () => void;
    onCancel?: () => void;
    children?: Snippet;
  }

  let {
    open = $bindable(false),
    title,
    description,
    confirmLabel = "confirm",
    cancelLabel = "cancel",
    tone = "default",
    onConfirm,
    onCancel,
    children,
  }: Props = $props();

  function close() {
    open = false;
    onCancel?.();
  }

  function confirm() {
    open = false;
    onConfirm?.();
  }

  function onKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") close();
    if (e.key === "Enter") confirm();
  }

  function focusOnMount(node: HTMLElement) {
    // Defer until rise animation has positioned the modal.
    requestAnimationFrame(() => node.focus());
  }
</script>

<svelte:window onkeydown={onKey} />

{#if open}
  <div class="backdrop" onclick={close} role="presentation">
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="modal-title"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      tabindex="-1"
      use:focusOnMount
    >
      <header class="head">
        <h2 id="modal-title">{title}</h2>
      </header>
      <div class="body">
        {#if description}
          <p class="desc">{description}</p>
        {/if}
        {@render children?.()}
      </div>
      <footer class="foot">
        <Button variant="ghost" onclick={close} title={cancelLabel}>
          {cancelLabel}
        </Button>
        <Button
          variant={tone === "danger" ? "danger" : "primary"}
          onclick={confirm}
          title={confirmLabel}
        >
          {confirmLabel}
        </Button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: var(--y7-bg-overlay);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: var(--y7-z-overlay);
    animation: fade var(--y7-dur-fast) var(--y7-ease);
  }
  .modal {
    min-width: 320px;
    max-width: 480px;
    /* Cap height so a long description can't push the footer buttons off
     * a short window; the body scrolls, head/foot stay pinned. */
    max-height: calc(100vh - var(--y7-sp-8) * 2);
    overflow: hidden;
    background: var(--y7-bg-elevated);
    border: 1px solid var(--y7-border-default);
    border-radius: var(--y7-r-lg);
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.55);
    display: flex;
    flex-direction: column;
    animation: rise var(--y7-dur-base) var(--y7-ease);
  }
  .head {
    padding: var(--y7-sp-4) var(--y7-sp-5) var(--y7-sp-2);
    border-bottom: 1px solid var(--y7-border-subtle);
  }
  h2 {
    margin: 0;
    font-size: var(--y7-fs-lg);
    font-weight: var(--y7-fw-semibold);
    color: var(--y7-text-primary);
    text-transform: lowercase;
    letter-spacing: 0.02em;
  }
  .body {
    padding: var(--y7-sp-4) var(--y7-sp-5);
    overflow-y: auto;
    min-height: 0;
  }
  .desc {
    margin: 0;
    color: var(--y7-text-secondary);
    font-size: var(--y7-fs-md);
    line-height: var(--y7-lh-relaxed);
  }
  .foot {
    display: flex;
    justify-content: flex-end;
    gap: var(--y7-sp-2);
    padding: var(--y7-sp-3) var(--y7-sp-5) var(--y7-sp-4);
    border-top: 1px solid var(--y7-border-subtle);
  }
  @keyframes fade {
    from { opacity: 0; }
    to { opacity: 1; }
  }
  @keyframes rise {
    from { opacity: 0; transform: translateY(8px); }
    to { opacity: 1; transform: translateY(0); }
  }
</style>
