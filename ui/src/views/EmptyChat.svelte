<script lang="ts">
  // Placeholder shown when no conversation is selected. Keep simple —
  // the user's own y7 lives in AddContact, not here.

  import Button from "../lib/components/Button.svelte";
  import { openAddContact } from "../lib/stores/route.svelte";
</script>

<section class="empty">
  <div class="inner">
    <p class="title">pick a contact, or add one to start.</p>
    <Button
      variant="ghost"
      size="sm"
      onclick={openAddContact}
      title="open the add-contact view"
    >
      add contact
    </Button>
  </div>
</section>

<style>
  .empty {
    flex: 1;
    min-height: 0;
    display: grid;
    place-items: center;
    padding: var(--y7-sp-8);
    background: var(--y7-bg-base);
  }
  .inner {
    text-align: center;
    max-width: 400px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--y7-sp-4);
  }
  .title {
    margin: 0;
    font-size: var(--y7-fs-md);
    color: var(--y7-text-secondary);
    text-transform: lowercase;
    letter-spacing: 0.02em;
  }
</style>
