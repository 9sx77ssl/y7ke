<script lang="ts">
  // Boot-window splash: the body renders this until `identity.isReady` flips
  // true and App.svelte swaps in MainShell. Keep it dead simple — a single
  // matte card, lowercase brand, a pulsing dot, an optional error line.

  import { identity } from "../lib/stores/identity.svelte";
  import KeyDisplay from "../lib/components/KeyDisplay.svelte";
  import StatusDot from "../lib/components/StatusDot.svelte";
</script>

<div class="setup">
  <div class="card">
    <header class="brand">
      <StatusDot
        tone={identity.error ? "error" : "connecting"}
        size={10}
        pulse={!identity.error}
        title={identity.error ? "error" : "starting"}
      />
      <span class="name">y7ke</span>
    </header>

    {#if identity.y7Id === null}
      <p class="status">generating identity…</p>
      {#if identity.error}
        <p class="error">{identity.error}</p>
      {/if}
    {:else}
      <KeyDisplay
        value={identity.y7Id}
        label="your identity"
        layout="block"
      />
      <p class="hint">
        share this with anyone you want to talk to. your private key never
        leaves this device.
      </p>
    {/if}
  </div>
</div>

<style>
  .setup {
    flex: 1;
    min-height: 0;
    display: grid;
    place-items: center;
    padding: var(--y7-sp-6);
    background: var(--y7-bg-base);
  }
  .card {
    width: min(420px, 100%);
    padding: var(--y7-sp-6);
    background: var(--y7-bg-elevated);
    border: 1px solid var(--y7-border-subtle);
    border-radius: var(--y7-r-lg);
    display: flex;
    flex-direction: column;
    gap: var(--y7-sp-4);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: var(--y7-sp-2);
    font-size: var(--y7-fs-xl);
    font-weight: var(--y7-fw-bold);
    color: var(--y7-text-primary);
    letter-spacing: 0.02em;
  }
  .name {
    text-transform: lowercase;
  }
  .status {
    margin: 0;
    font-size: var(--y7-fs-md);
    color: var(--y7-text-secondary);
    text-transform: lowercase;
    letter-spacing: 0.02em;
  }
  .hint {
    margin: 0;
    font-size: var(--y7-fs-sm);
    line-height: var(--y7-lh-relaxed);
    color: var(--y7-text-muted);
    text-transform: lowercase;
  }
  .error {
    margin: 0;
    font-size: var(--y7-fs-sm);
    color: var(--y7-red);
  }
</style>
