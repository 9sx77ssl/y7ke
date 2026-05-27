<script lang="ts">
  // Textarea with two opinionated defaults vs. the native element:
  //   1. resize handle disabled — vertical drag-resize is jarring inside a
  //      Tauri chat window and produces the "infinite growing input" bug.
  //   2. content-driven auto-grow up to `maxRows` lines, after which the
  //      element keeps its capped height and the user scrolls inside it
  //      (Telegram / Slack composer behaviour).
  //
  // Pass `autosize={false}` to opt out (e.g. for forms where the field is
  // already a fixed `rows` block such as a greeting textarea).

  interface Props {
    value: string;
    placeholder?: string;
    disabled?: boolean;
    invalid?: boolean;
    /** Initial visible rows when empty. Default 1 for chat-like composers. */
    rows?: number;
    /** Cap (in rows) at which auto-grow stops and internal scroll begins. */
    maxRows?: number;
    /** Disable height-on-input recalculation. */
    autosize?: boolean;
    ariaLabel?: string;
    spellcheck?: boolean;
    oninput?: (e: Event) => void;
    onkeydown?: (e: KeyboardEvent) => void;
  }

  let {
    value = $bindable(""),
    placeholder,
    disabled = false,
    invalid = false,
    rows = 1,
    maxRows = 5,
    autosize = true,
    ariaLabel,
    spellcheck = false,
    oninput,
    onkeydown,
  }: Props = $props();

  let el = $state<HTMLTextAreaElement | null>(null);

  function lineHeightPx(node: HTMLTextAreaElement): number {
    const lh = window.getComputedStyle(node).lineHeight;
    const parsed = parseFloat(lh);
    if (Number.isFinite(parsed) && parsed > 0) return parsed;
    // Fallback for `normal` line-height.
    return parseFloat(window.getComputedStyle(node).fontSize) * 1.5;
  }

  function paddingPx(node: HTMLTextAreaElement): number {
    const cs = window.getComputedStyle(node);
    return parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom);
  }

  function recalc(): void {
    if (!autosize || !el) return;
    // Shrink first so scrollHeight reflects content, not the previous height.
    el.style.height = "auto";
    const lh = lineHeightPx(el);
    const pad = paddingPx(el);
    const minPx = lh * Math.max(rows, 1) + pad;
    const capPx = lh * maxRows + pad;
    const desired = Math.min(Math.max(el.scrollHeight, minPx), capPx);
    el.style.height = `${desired}px`;
    // When content needs more than capPx, internal scroll kicks in.
    el.style.overflowY = el.scrollHeight > capPx ? "auto" : "hidden";
  }

  // Recalc on mount (when `el` is bound) and on every value change.
  $effect(() => {
    void value;
    if (el) {
      queueMicrotask(recalc);
    }
  });
</script>

<textarea
  bind:this={el}
  bind:value
  {rows}
  {placeholder}
  {disabled}
  {spellcheck}
  aria-label={ariaLabel}
  aria-invalid={invalid}
  class="ta"
  class:invalid
  class:no-autosize={!autosize}
  oninput={(e) => {
    oninput?.(e);
    recalc();
  }}
  onkeydown={(e) => onkeydown?.(e)}
></textarea>

<style>
  .ta {
    width: 100%;
    /* Auto-grow handles vertical sizing; native resize handle removed. */
    min-height: var(--y7-sz-input);
    padding: var(--y7-sp-2) var(--y7-sp-3);
    background: var(--y7-bg-base);
    border: 1px solid var(--y7-border-default);
    border-radius: var(--y7-r-md);
    color: var(--y7-text-primary);
    font-family: var(--y7-font-mono);
    font-size: var(--y7-fs-md);
    line-height: var(--y7-lh-normal);
    resize: none;
    overflow-y: hidden;
    transition: border-color var(--y7-dur-fast) var(--y7-ease);
  }
  .ta.no-autosize {
    /* Fixed-rows variant: keep the native rows, but still hide the resize
     * handle so a user can't drag the field to absurd sizes. */
    overflow-y: auto;
  }
  .ta::placeholder {
    color: var(--y7-text-muted);
  }
  .ta:hover:not(:disabled) {
    border-color: var(--y7-border-strong);
  }
  .ta:focus {
    outline: none;
    border-color: var(--y7-border-focus);
  }
  .ta.invalid {
    border-color: var(--y7-red-dim);
  }
  .ta:disabled {
    opacity: 0.5;
  }
</style>
