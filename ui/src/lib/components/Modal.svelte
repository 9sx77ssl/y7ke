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

  // Element to return focus to when the modal closes (the trigger).
  let returnFocus: HTMLElement | null = null;

  function restoreFocus() {
    returnFocus?.focus?.();
    returnFocus = null;
  }

  function close() {
    open = false;
    restoreFocus();
    onCancel?.();
  }

  function confirm() {
    open = false;
    restoreFocus();
    onConfirm?.();
  }

  function onKey(e: KeyboardEvent) {
    // Only Escape is handled globally. Enter is intentionally NOT a global
    // confirm — that let a stray Enter fire a destructive action; instead it
    // activates whichever button is focused (cancel is focused first).
    if (open && e.key === "Escape") close();
  }

  const FOCUSABLE =
    'button:not([disabled]), [href], input:not([disabled]), [tabindex]:not([tabindex="-1"])';

  function focusOnMount(node: HTMLElement) {
    returnFocus = (document.activeElement as HTMLElement | null) ?? null;
    requestAnimationFrame(() => {
      // Focus the first control (the cancel button) so Enter is safe.
      const first = node.querySelector<HTMLElement>(FOCUSABLE);
      (first ?? node).focus();
    });
  }

  // Trap Tab within the dialog so focus can't escape behind the backdrop.
  function onDialogKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key !== "Tab") return;
    const dialog = e.currentTarget as HTMLElement;
    const nodes = Array.from(
      dialog.querySelectorAll<HTMLElement>(FOCUSABLE),
    ).filter((el) => el.offsetParent !== null);
    if (nodes.length === 0) return;
    const first = nodes[0];
    const last = nodes[nodes.length - 1];
    const active = document.activeElement as HTMLElement | null;
    if (e.shiftKey && active === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first.focus();
    }
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
      onkeydown={onDialogKey}
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
