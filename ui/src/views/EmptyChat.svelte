<script lang="ts">
  import { identity } from "../lib/stores/identity.svelte";
  import { truncateY7Id } from "../lib/format";

  async function copyMyId(): Promise<void> {
    if (identity.y7Id === null) return;
    try {
      await navigator.clipboard.writeText(identity.y7Id);
    } catch {
      // Silent — the IdentitySetup view is responsible for the copy UX.
    }
  }
</script>

<section class="empty">
  <div class="inner">
    <p class="title">Pick a contact, or add one to start.</p>
    {#if identity.y7Id !== null}
      <p class="me">
        Your ID:
        <code title={identity.y7Id}>{truncateY7Id(identity.y7Id, 10, 8)}</code>
        <button type="button" onclick={copyMyId}>Copy</button>
      </p>
    {/if}
  </div>
</section>

<style>
  .empty {
    height: 100%;
    display: grid;
    place-items: center;
    padding: 2rem;
    color: color-mix(in oklab, currentColor 80%, transparent);
  }
  .inner {
    text-align: center;
    max-width: 28rem;
  }
  .title {
    font-size: 1rem;
    margin: 0 0 1rem;
    opacity: 0.65;
  }
  .me {
    margin: 0;
    font-size: 0.85rem;
    opacity: 0.6;
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
  }
  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.8rem;
    padding: 0.15rem 0.4rem;
    background: color-mix(in oklab, currentColor 6%, transparent);
    border-radius: 4px;
  }
  button {
    font: inherit;
    font-size: 0.8rem;
    padding: 0.25rem 0.6rem;
    border-radius: 5px;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  button:hover {
    background: color-mix(in oklab, currentColor 8%, transparent);
  }
</style>
