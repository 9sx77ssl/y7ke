<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title?: string;
    children?: Snippet;
    actions?: Snippet;
    padded?: boolean;
  }

  let { title, children, actions, padded = true }: Props = $props();
</script>

<section class="card">
  {#if title || actions}
    <header class="head">
      {#if title}<h3 class="title">{title}</h3>{/if}
      {#if actions}<div class="actions">{@render actions()}</div>{/if}
    </header>
  {/if}
  <div class="body" class:padded>
    {@render children?.()}
  </div>
</section>

<style>
  .card {
    background: var(--y7-bg-elevated);
    border: 1px solid var(--y7-border-subtle);
    border-radius: var(--y7-r-lg);
    overflow: hidden;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--y7-sp-3);
    padding: var(--y7-sp-3) var(--y7-sp-4);
    border-bottom: 1px solid var(--y7-border-subtle);
  }
  .title {
    margin: 0;
    font-size: var(--y7-fs-md);
    font-weight: var(--y7-fw-semibold);
    color: var(--y7-text-secondary);
    text-transform: lowercase;
    letter-spacing: 0.02em;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--y7-sp-2);
  }
  .body.padded {
    padding: var(--y7-sp-4);
  }
</style>
