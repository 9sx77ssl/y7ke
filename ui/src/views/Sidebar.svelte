<script lang="ts">
  // Fixed-width sidebar: brand row (no y7 ID — the user copies it from the
  // AddContact view, this row is identity-only), nav buttons, contacts list.

  import {
    contacts,
    deleteContactAction,
    refreshContacts,
  } from "../lib/stores/contacts.svelte";
  import { refreshRequests, requests } from "../lib/stores/requests.svelte";
  import { getPresence, presenceLabel } from "../lib/stores/presence.svelte";
  import {
    openAddContact,
    openChatWith,
    openEmpty,
    openRequests,
    openSettings,
    openConnectivity,
    router,
  } from "../lib/stores/route.svelte";
  import { truncateY7Id } from "../lib/format";
  import { toast } from "../lib/components/toast.svelte";
  import { log } from "../lib/log";
  import type { ConnectionKind } from "../lib/types";
  import NavItem from "../lib/components/NavItem.svelte";
  import ContactRow from "../lib/components/ContactRow.svelte";
  import ContextMenu, {
    type ContextMenuItem,
  } from "../lib/components/ContextMenu.svelte";
  import Modal from "../lib/components/Modal.svelte";
  import type { StatusTone } from "../lib/components/StatusDot.svelte";

  const logger = log("Sidebar");

  // Context menu state
  let ctxOpen = $state(false);
  let ctxX = $state(0);
  let ctxY = $state(0);
  let ctxItems = $state<ContextMenuItem[]>([]);

  // Delete-confirm modal
  let deleteOpen = $state(false);
  let deleteTarget = $state<string | null>(null);

  function openContextMenu(e: MouseEvent, y7Id: string) {
    e.preventDefault();
    e.stopPropagation();
    ctxX = e.clientX;
    ctxY = e.clientY;
    ctxItems = [
      {
        label: "delete chat",
        danger: true,
        onClick: () => {
          deleteTarget = y7Id;
          deleteOpen = true;
        },
      },
    ];
    ctxOpen = true;
  }

  async function confirmDelete() {
    const target = deleteTarget;
    if (target === null) return;
    logger.info("deleting contact", target);
    try {
      await deleteContactAction(target);
      toast.success("chat deleted");
      if (router.pane.kind === "chat" && router.pane.peerY7Id === target) {
        openEmpty();
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      logger.error("delete failed", msg);
      toast.error(`delete failed: ${msg}`);
    } finally {
      deleteTarget = null;
    }
  }

  $effect(() => {
    if (!contacts.loadedOnce && !contacts.loading) void refreshContacts();
    if (!requests.loadedOnce && !requests.loading) void refreshRequests();
  });

  function isOpen(y7Id: string): boolean {
    return router.pane.kind === "chat" && router.pane.peerY7Id === y7Id;
  }

  function dotTone(p: ConnectionKind): StatusTone {
    switch (p) {
      case "lan":
      case "internet":
      case "direct":
      case "relayed":
        return "online";
      case "connecting":
        return "connecting";
      case "offline":
        return "offline";
    }
  }

  function pendingLabel(status: string): string | null {
    switch (status) {
      case "pending_out":
        return "pending — waiting for accept";
      case "pending_in":
        return "pending — accept in requests";
      default:
        return null;
    }
  }
</script>

<aside class="sidebar">
  <nav class="actions" aria-label="primary">
    <NavItem
      label="add contact ^.^"
      title="add a new contact"
      active={router.pane.kind === "add_contact"}
      onclick={openAddContact}
    />
    <NavItem
      label="requests >.<"
      title="view pending contact requests"
      badge={requests.incomingCount}
      active={router.pane.kind === "requests"}
      onclick={openRequests}
    />
    <NavItem
      label="settings :3"
      title="connection modes and bootstrap nodes"
      active={router.pane.kind === "settings"}
      onclick={openSettings}
    />
    <NavItem
      label="connectivity O.O"
      title="live nat / dcutr / transport diagnostics"
      active={router.pane.kind === "connectivity"}
      onclick={openConnectivity}
    />
  </nav>

  <div class="section-head">
    <span class="section-title">contacts</span>
  </div>

  <ul class="contacts">
    {#if contacts.visible.length === 0}
      <li class="empty">
        {#if contacts.loading}
          loading…
        {:else if contacts.error}
          <span class="err">{contacts.error}</span>
        {:else}
          no contacts yet.
        {/if}
      </li>
    {/if}

    {#each contacts.visible as c (c.y7_id)}
      {@const presence = getPresence(c.y7_id)}
      {@const pending = pendingLabel(c.status)}
      <li
        oncontextmenu={(e) => openContextMenu(e, c.y7_id)}
        role="presentation"
      >
        <ContactRow
          label={c.nickname ?? truncateY7Id(c.y7_id, 8, 6)}
          sublabel={pending ?? (c.nickname ? truncateY7Id(c.y7_id, 6, 4) : undefined)}
          presence={pending ? "connecting" : dotTone(presence)}
          connectionKind={pending ? undefined : presence}
          title="{c.y7_id}{pending ? ` — ${pending}` : ` — ${presenceLabel(presence)}`}"
          active={isOpen(c.y7_id)}
          onclick={() => openChatWith(c.y7_id)}
        />
      </li>
    {/each}
  </ul>

  <div class="footer" aria-hidden="true">
    {#if contacts.visible.length > 0}
      <span class="count">
        {contacts.visible.length}
        {contacts.visible.length === 1 ? "contact" : "contacts"}
      </span>
    {/if}
  </div>
</aside>

<ContextMenu bind:open={ctxOpen} x={ctxX} y={ctxY} items={ctxItems} />

<Modal
  bind:open={deleteOpen}
  title="delete chat"
  description="permanently deletes messages, session key, and contact on both sides — your copy is wiped now, and the peer's copy is wiped automatically the next time they come online."
  confirmLabel="delete"
  cancelLabel="cancel"
  tone="danger"
  onConfirm={confirmDelete}
/>

<style>
  .sidebar {
    width: var(--y7-sz-sidebar);
    flex-shrink: 0;
    background: var(--y7-bg-sidebar);
    border-right: 1px solid var(--y7-border-subtle);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .actions {
    display: flex;
    flex-direction: column;
    gap: var(--y7-sp-1);
    padding: var(--y7-sp-4) var(--y7-sp-2) var(--y7-sp-3);
  }

  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--y7-sp-3) var(--y7-sp-4) var(--y7-sp-2);
    border-top: 1px solid var(--y7-border-subtle);
    margin-top: var(--y7-sp-2);
  }
  .section-title {
    font-size: var(--y7-fs-xs);
    color: var(--y7-text-muted);
    text-transform: lowercase;
    letter-spacing: 0.08em;
  }

  .contacts {
    list-style: none;
    margin: 0;
    padding: 0 var(--y7-sp-2) var(--y7-sp-3);
    overflow-y: auto;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .empty {
    padding: var(--y7-sp-3) var(--y7-sp-3);
    font-size: var(--y7-fs-sm);
    color: var(--y7-text-muted);
    text-transform: lowercase;
  }
  .err {
    color: var(--y7-red);
  }

  .footer {
    padding: var(--y7-sp-2) var(--y7-sp-4);
    border-top: 1px solid var(--y7-border-subtle);
    font-size: var(--y7-fs-xs);
    color: var(--y7-text-muted);
    text-transform: lowercase;
    letter-spacing: 0.04em;
    min-height: 26px;
    display: flex;
    align-items: center;
  }
</style>
