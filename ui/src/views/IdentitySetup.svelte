<script lang="ts">
  import { identity } from "../lib/stores/identity.svelte";
  import { truncateY7Id } from "../lib/format";

  let copied = $state(false);
  let copyError = $state<string | null>(null);

  async function copyId(): Promise<void> {
    const id = identity.y7Id;
    if (id === null) return;
    try {
      await navigator.clipboard.writeText(id);
      copied = true;
      copyError = null;
      window.setTimeout(() => {
        copied = false;
      }, 1600);
    } catch (err) {
      copyError = err instanceof Error ? err.message : String(err);
    }
  }
</script>

<main class="setup">
  <div class="card">
    <div class="brand">Y7KE</div>

    {#if identity.y7Id === null}
      <p class="status">Generating identity…</p>
      {#if identity.error}
        <p class="error">{identity.error}</p>
      {/if}
    {:else}
      <p class="status">Your Y7KE identity</p>
      <div class="id-row">
        <code class="id" title={identity.y7Id}>{truncateY7Id(identity.y7Id, 12, 10)}</code>
        <button type="button" onclick={copyId} aria-label="Copy identity to clipboard">
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <p class="hint">
        Share this string with anyone you want to talk to. Your private key
        never leaves this device.
      </p>
      {#if copyError}
        <p class="error">Clipboard error: {copyError}</p>
      {/if}
    {/if}
  </div>
</main>

<style>
  .setup {
    min-height: 100vh;
    display: grid;
    place-items: center;
    padding: 2rem;
    font-family:
      ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  }
  .card {
    max-width: 28rem;
    width: 100%;
    border: 1px solid color-mix(in oklab, currentColor 16%, transparent);
    border-radius: 10px;
    padding: 2rem 1.75rem;
    background: color-mix(in oklab, Canvas 100%, currentColor 2%);
  }
  .brand {
    font-size: 1.25rem;
    font-weight: 600;
    letter-spacing: -0.01em;
    margin-bottom: 1.25rem;
    opacity: 0.85;
  }
  .status {
    margin: 0 0 0.75rem;
    font-size: 0.95rem;
    opacity: 0.75;
  }
  .id-row {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin-bottom: 0.75rem;
  }
  .id {
    flex: 1;
    padding: 0.6rem 0.75rem;
    border-radius: 6px;
    background: color-mix(in oklab, currentColor 6%, transparent);
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.85rem;
    overflow-x: auto;
    white-space: nowrap;
  }
  button {
    font: inherit;
    padding: 0.55rem 0.95rem;
    border-radius: 6px;
    border: 1px solid color-mix(in oklab, currentColor 22%, transparent);
    background: color-mix(in oklab, Canvas 100%, currentColor 4%);
    color: inherit;
    cursor: pointer;
  }
  button:hover {
    background: color-mix(in oklab, currentColor 8%, transparent);
  }
  .hint {
    margin: 0;
    font-size: 0.8rem;
    opacity: 0.6;
    line-height: 1.45;
  }
  .error {
    margin: 0.5rem 0 0;
    color: color-mix(in oklab, currentColor 70%, crimson);
    font-size: 0.85rem;
  }
</style>
