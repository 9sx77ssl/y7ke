<script lang="ts">
  // Chip-style toggle. Active = filled accent; inactive = outlined.

  interface Props {
    label: string;
    checked: boolean;
    disabled?: boolean;
    onchange: (next: boolean) => void;
  }
  let { label, checked, disabled = false, onchange }: Props = $props();
</script>

<button
  type="button"
  class="toggle"
  class:active={checked}
  class:disabled
  {disabled}
  aria-pressed={checked}
  onclick={() => onchange(!checked)}
>
  {label}
</button>

<style>
  .toggle {
    display: inline-flex;
    align-items: center;
    height: var(--y7-sz-btn-md);
    padding: 0 var(--y7-sp-4);
    border-radius: var(--y7-r-full);
    border: 1px solid var(--y7-border-strong);
    background: transparent;
    color: var(--y7-text-secondary);
    font-family: var(--y7-font-mono);
    font-size: var(--y7-fs-md);
    line-height: 1;
    text-transform: lowercase;
    letter-spacing: 0.04em;
    cursor: pointer;
    transition:
      background-color var(--y7-dur-fast) var(--y7-ease),
      border-color var(--y7-dur-fast) var(--y7-ease),
      color var(--y7-dur-fast) var(--y7-ease);
  }
  .toggle:hover:not(:disabled) {
    background: var(--y7-bg-hover);
    color: var(--y7-text-primary);
  }
  .toggle.active {
    background: var(--y7-text-primary);
    border-color: var(--y7-text-primary);
    color: var(--y7-text-on-accent);
  }
  .toggle.active:hover:not(:disabled) {
    background: var(--y7-text-primary);
    color: var(--y7-text-on-accent);
    opacity: 0.9;
  }
  .toggle.disabled,
  .toggle:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
</style>
