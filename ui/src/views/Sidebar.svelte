<script lang="ts">
  import {
    contacts,
    refreshContacts,
  } from "../lib/stores/contacts.svelte";
  import { refreshRequests, requests } from "../lib/stores/requests.svelte";
  import { identity } from "../lib/stores/identity.svelte";
  import { getPresence, presenceLabel } from "../lib/stores/presence.svelte";
  import {
    openAddContact,
    openChatWith,
    openRequests,
    router,
  } from "../lib/stores/route.svelte";
  import { truncateY7Id } from "../lib/format";
  import type { ConnectionKind } from "../lib/types";

  $effect(() => {
    if (!contacts.loadedOnce && !contacts.loading) void refreshContacts();
    if (!requests.loadedOnce && !requests.loading) void refreshRequests();
  });

  function isOpen(y7Id: string): boolean {
    return router.pane.kind === "chat" && router.pane.peerY7Id === y7Id;
  }

  function presenceDotClass(p: ConnectionKind): string {
    return `dot ${p}`;
  }

  async function copyMyId(): Promise<void> {
    if (identity.y7Id === null) return;
    try {
      await navigator.clipboard.writeText(identity.y7Id);
    } catch {
      /* ignore */
    }
  }
</script>

<aside class="sidebar">
  <header class="brand-row">
    <span class="brand">Y7KE</span>
    {#if identity.y7Id !== null}
      <button
        type="button"
        class="me"
        onclick={copyMyId}
        title={`Copy ${identity.y7Id}`}
        aria-label="Copy your identity"
      >
        <code>{truncateY7Id(identity.y7Id, 6, 4)}</code>
      </button>
    {/if}
  </header>

  <nav class="actions">
    <button
      type="button"
      class="primary"
      onclick={openAddContact}
      data-active={router.pane.kind === "add_contact"}
    >
      <span class="plus" aria-hidden="true">+</span>
      Add contact
    </button>
    <button
      type="button"
      class="link"
      onclick={openRequests}
      data-active={router.pane.kind === "requests"}
    >
      Requests
      {#if requests.incomingCount > 0}
        <span class="badge">{requests.incomingCount}</span>
      {/if}
    </button>
  </nav>

  <div class="section-head">
    <span>Contacts</span>
    <button
      type="button"
      class="icon"
      onclick={() => {
        void refreshContacts();
      }}
      disabled={contacts.loading}
      aria-label="Refresh contacts"
      title="Refresh"
    >
      ↻
    </button>
  </div>

  <ul class="contacts">
    {#if contacts.accepted.length === 0}
      <li class="empty">
        {#if contacts.loading}
          Loading…
        {:else if contacts.error}
          <span class="err">{contacts.error}</span>
        {:else}
          No contacts yet.
        {/if}
      </li>
    {/if}

    {#each contacts.accepted as c (c.y7_id)}
      {@const presence = getPresence(c.y7_id)}
      <li>
        <button
          type="button"
          class="contact"
          data-active={isOpen(c.y7_id)}
          onclick={() => openChatWith(c.y7_id)}
        >
          <span
            class={presenceDotClass(presence)}
            aria-label={`Status: ${presenceLabel(presence)}`}
            title={presenceLabel(presence)}
          ></span>
          <span class="contact-meta">
            <span class="contact-name">
              {c.nickname ?? truncateY7Id(c.y7_id, 8, 6)}
            </span>
            <code class="contact-id" title={c.y7_id}>
              {truncateY7Id(c.y7_id, 6, 4)}
            </code>
          </span>
        </button>
      </li>
    {/each}
  </ul>
</aside>

<style>
  .sidebar {
    width: 260px;
    border-right: 1px solid color-mix(in oklab, currentColor 12%, transparent);
    background: color-mix(in oklab, Canvas 100%, currentColor 2.5%);
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .brand-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.95rem 1rem 0.75rem;
    border-bottom: 1px solid color-mix(in oklab, currentColor 10%, transparent);
  }
  .brand {
    font-weight: 600;
    font-size: 0.95rem;
    letter-spacing: -0.01em;
  }
  .me {
    font: inherit;
    background: transparent;
    border: none;
    padding: 0.15rem 0.3rem;
    border-radius: 4px;
    cursor: pointer;
    color: inherit;
    opacity: 0.65;
  }
  .me:hover {
    background: color-mix(in oklab, currentColor 8%, transparent);
    opacity: 1;
  }
  .me code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.72rem;
  }
  .actions {
    padding: 0.75rem 0.75rem 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }
  .actions button {
    font: inherit;
    text-align: left;
    border-radius: 6px;
    border: 1px solid transparent;
    background: transparent;
    color: inherit;
    cursor: pointer;
    padding: 0.5rem 0.65rem;
    font-size: 0.88rem;
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .actions button.primary {
    border-color: color-mix(in oklab, currentColor 18%, transparent);
    background: color-mix(in oklab, currentColor 6%, transparent);
  }
  .actions button:hover {
    background: color-mix(in oklab, currentColor 10%, transparent);
  }
  .actions button[data-active="true"] {
    background: color-mix(in oklab, AccentColor 22%, transparent);
    border-color: color-mix(in oklab, AccentColor 30%, transparent);
  }
  .plus {
    font-weight: 700;
    opacity: 0.7;
  }
  .badge {
    margin-left: auto;
    background: color-mix(in oklab, AccentColor 50%, transparent);
    color: Canvas;
    font-size: 0.7rem;
    border-radius: 999px;
    padding: 0.05rem 0.45rem;
    min-width: 1.25rem;
    text-align: center;
  }
  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.85rem 1rem 0.4rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    opacity: 0.55;
  }
  button.icon {
    font: inherit;
    background: transparent;
    border: none;
    cursor: pointer;
    color: inherit;
    opacity: 0.65;
    padding: 0.1rem 0.35rem;
    border-radius: 4px;
    font-size: 0.9rem;
  }
  button.icon:hover {
    background: color-mix(in oklab, currentColor 8%, transparent);
    opacity: 1;
  }
  .contacts {
    list-style: none;
    margin: 0;
    padding: 0 0.5rem 1rem;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }
  .contacts .empty {
    padding: 0.75rem 0.5rem;
    font-size: 0.82rem;
    opacity: 0.55;
  }
  .err {
    color: color-mix(in oklab, currentColor 70%, crimson);
  }
  .contact {
    width: 100%;
    text-align: left;
    font: inherit;
    background: transparent;
    border: none;
    cursor: pointer;
    color: inherit;
    padding: 0.5rem 0.55rem;
    border-radius: 6px;
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .contact:hover {
    background: color-mix(in oklab, currentColor 8%, transparent);
  }
  .contact[data-active="true"] {
    background: color-mix(in oklab, AccentColor 22%, transparent);
  }
  .contact-meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .contact-name {
    font-size: 0.88rem;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .contact-id {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.7rem;
    opacity: 0.55;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    background: currentColor;
    opacity: 0.45;
  }
  .dot.lan {
    background: color-mix(in oklab, currentColor 40%, seagreen);
    opacity: 1;
  }
  .dot.connecting {
    background: color-mix(in oklab, currentColor 40%, goldenrod);
    opacity: 1;
  }
  .dot.offline {
    opacity: 0.35;
  }
</style>
