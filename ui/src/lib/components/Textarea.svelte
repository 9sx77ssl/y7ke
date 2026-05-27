<script lang="ts">
  interface Props {
    value: string;
    placeholder?: string;
    disabled?: boolean;
    invalid?: boolean;
    rows?: number;
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
    rows = 3,
    ariaLabel,
    spellcheck = false,
    oninput,
    onkeydown,
  }: Props = $props();
</script>

<textarea
  bind:value
  {rows}
  {placeholder}
  {disabled}
  {spellcheck}
  aria-label={ariaLabel}
  aria-invalid={invalid}
  class="ta"
  class:invalid
  oninput={(e) => oninput?.(e)}
  onkeydown={(e) => onkeydown?.(e)}
></textarea>

<style>
  .ta {
    width: 100%;
    min-height: calc(var(--y7-sz-input) * 2);
    padding: var(--y7-sp-2) var(--y7-sp-3);
    background: var(--y7-bg-base);
    border: 1px solid var(--y7-border-default);
    border-radius: var(--y7-r-md);
    color: var(--y7-text-primary);
    font-family: var(--y7-font-mono);
    font-size: var(--y7-fs-md);
    line-height: var(--y7-lh-normal);
    resize: vertical;
    transition: border-color var(--y7-dur-fast) var(--y7-ease);
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
