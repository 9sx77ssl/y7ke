<script lang="ts">
  interface Props {
    value: string;
    type?: "text" | "password";
    placeholder?: string;
    disabled?: boolean;
    invalid?: boolean;
    ariaLabel?: string;
    spellcheck?: boolean;
    autocomplete?: string;
    oninput?: (e: Event) => void;
    onkeydown?: (e: KeyboardEvent) => void;
  }

  let {
    value = $bindable(""),
    type = "text",
    placeholder,
    disabled = false,
    invalid = false,
    ariaLabel,
    spellcheck = false,
    autocomplete = "off",
    oninput,
    onkeydown,
  }: Props = $props();
</script>

<input
  bind:value
  {type}
  {placeholder}
  {disabled}
  aria-label={ariaLabel}
  aria-invalid={invalid}
  {spellcheck}
  {autocomplete}
  class="input"
  class:invalid
  oninput={(e) => oninput?.(e)}
  onkeydown={(e) => onkeydown?.(e)}
/>

<style>
  .input {
    width: 100%;
    height: var(--y7-sz-input);
    padding: 0 var(--y7-sp-3);
    background: var(--y7-bg-base);
    border: 1px solid var(--y7-border-default);
    border-radius: var(--y7-r-md);
    color: var(--y7-text-primary);
    font-family: var(--y7-font-mono);
    font-size: var(--y7-fs-md);
    line-height: 1;
    transition: border-color var(--y7-dur-fast) var(--y7-ease);
  }
  .input::placeholder {
    color: var(--y7-text-muted);
  }
  .input:hover:not(:disabled) {
    border-color: var(--y7-border-strong);
  }
  .input:focus {
    outline: none;
    border-color: var(--y7-border-focus);
  }
  .input.invalid {
    border-color: var(--y7-red-dim);
  }
  .input:disabled {
    opacity: 0.5;
  }
</style>
