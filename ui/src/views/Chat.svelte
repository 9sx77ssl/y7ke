<script lang="ts">
  // Active conversation view. Top: peer header (nickname + click-to-copy y7).
  // Middle: scrollable message list. Bottom: composer (Textarea + Send).
  // Re-opens the conversation whenever the peerY7Id prop changes (sidebar
  // navigation triggers this via {#key} in MainShell).

  import {
    chat,
    openConversation,
    sendText,
  } from "../lib/stores/chat.svelte";
  import { findContact } from "../lib/stores/contacts.svelte";
  import { getPresence, presenceLabel } from "../lib/stores/presence.svelte";
  import { truncateY7Id } from "../lib/format";
  import { log } from "../lib/log";
  import Button from "../lib/components/Button.svelte";
  import KeyDisplay from "../lib/components/KeyDisplay.svelte";
  import MessageBubble from "../lib/components/MessageBubble.svelte";
  import StatusDot from "../lib/components/StatusDot.svelte";
  import Textarea from "../lib/components/Textarea.svelte";
  import type { ConnectionKind } from "../lib/types";
  import type { StatusTone } from "../lib/components/StatusDot.svelte";

  interface Props {
    peerY7Id: string;
  }

  let { peerY7Id }: Props = $props();
  const logger = log("Chat");

  let composer = $state("");
  let scrollEl = $state<HTMLDivElement | undefined>(undefined);
  let lastSeenCount = 0;

  // B4: always re-open on mount or peer change. Store state can be stale
  // across route navigations.
  $effect(() => {
    logger.debug("opening conversation", peerY7Id);
    void openConversation(peerY7Id);
  });

  // Auto-scroll to bottom whenever a new message appears or we just opened.
  $effect(() => {
    const count = chat.messages.length;
    if (!scrollEl) return;
    if (count !== lastSeenCount) {
      lastSeenCount = count;
      queueMicrotask(() => {
        if (scrollEl) {
          scrollEl.scrollTop = scrollEl.scrollHeight;
        }
      });
    }
  });

  const contact = $derived(findContact(peerY7Id));
  const presence = $derived(getPresence(peerY7Id));
  const displayName = $derived(
    contact?.nickname ?? truncateY7Id(peerY7Id, 10, 8),
  );
  const hasNickname = $derived(contact?.nickname != null);

  async function submit(): Promise<void> {
    const text = composer.trim();
    if (text.length === 0 || chat.sending) return;
    composer = "";
    logger.debug("sending message", { to: peerY7Id, len: text.length });
    await sendText(text);
  }

  function handleKeydown(ev: KeyboardEvent): void {
    if (ev.key === "Enter" && !ev.shiftKey) {
      ev.preventDefault();
      void submit();
    }
  }

  function presenceTone(p: ConnectionKind): StatusTone {
    switch (p) {
      case "lan":
        return "online";
      case "connecting":
        return "connecting";
      case "offline":
        return "offline";
    }
  }
</script>

<section class="chat">
  <header class="head">
    <div class="who">
      <StatusDot
        tone={presenceTone(presence)}
        size={8}
        title={presenceLabel(presence)}
      />
      <span class="name" title={contact?.nickname ?? peerY7Id}>
        {displayName}
      </span>
      <span class="presence">{presenceLabel(presence).toLowerCase()}</span>
    </div>
    {#if hasNickname}
      <div class="head-right">
        <KeyDisplay value={peerY7Id} layout="inline" truncate />
      </div>
    {/if}
  </header>

  <div class="scroll" bind:this={scrollEl}>
    <div class="scroll-inner">
      {#if chat.loading && chat.messages.length === 0}
        <p class="info">loading…</p>
      {/if}

      {#if chat.error}
        <p class="info err">{chat.error}</p>
      {/if}

      {#if !chat.loading && chat.messages.length === 0 && !chat.error}
        <p class="info">
          no messages yet. say something — it'll deliver as soon as
          {displayName} is on the lan.
        </p>
      {/if}

      <ol class="list">
        {#each chat.messages as msg (msg.message_id)}
          <li>
            <MessageBubble
              text={msg.text}
              timestampMs={msg.timestamp_ms}
              isMine={msg.is_mine}
              status={msg.status}
            />
          </li>
        {/each}
      </ol>
    </div>
  </div>

  <form
    class="composer"
    onsubmit={(ev) => {
      ev.preventDefault();
      void submit();
    }}
  >
    <Textarea
      bind:value={composer}
      placeholder="write a message…"
      rows={1}
      maxRows={5}
      disabled={chat.sending}
      ariaLabel="message"
      onkeydown={handleKeydown}
    />
    <Button
      type="submit"
      variant="primary"
      disabled={chat.sending || composer.trim().length === 0}
      title="send message (enter)"
    >
      {chat.sending ? "sending…" : "send"}
    </Button>
  </form>
</section>

<style>
  .chat {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: grid;
    grid-template-rows: auto 1fr auto;
    background: var(--y7-bg-base);
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--y7-sp-3);
    padding: var(--y7-sp-3) var(--y7-sp-5);
    border-bottom: 1px solid var(--y7-border-subtle);
    background: var(--y7-bg-sidebar);
    min-width: 0;
  }
  .who {
    display: flex;
    align-items: center;
    gap: var(--y7-sp-2);
    min-width: 0;
    flex: 1;
  }
  .name {
    font-weight: var(--y7-fw-semibold);
    font-size: var(--y7-fs-lg);
    color: var(--y7-text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .presence {
    font-size: var(--y7-fs-xs);
    color: var(--y7-text-muted);
    text-transform: lowercase;
    letter-spacing: 0.04em;
    white-space: nowrap;
  }
  .head-right {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: var(--y7-sp-2);
    min-width: 0;
  }

  .scroll {
    overflow-y: auto;
    min-height: 0;
    padding: var(--y7-sp-4) var(--y7-sp-5);
  }
  .scroll-inner {
    max-width: 760px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: var(--y7-sp-3);
  }
  .info {
    margin: var(--y7-sp-4) 0;
    text-align: center;
    font-size: var(--y7-fs-sm);
    color: var(--y7-text-muted);
    text-transform: lowercase;
    letter-spacing: 0.02em;
  }
  .info.err {
    color: var(--y7-red);
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--y7-sp-2);
  }
  .list li {
    display: flex;
    flex-direction: column;
  }

  .composer {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: end;
    gap: var(--y7-sp-2);
    padding: var(--y7-sp-3) var(--y7-sp-5);
    border-top: 1px solid var(--y7-border-subtle);
    background: var(--y7-bg-sidebar);
  }
  /* Match the send button's height to the textarea's first-line height so
     they line up flush. Textarea's auto-grow puts it above --y7-sz-btn-md. */
  .composer :global(.btn) {
    height: auto;
    align-self: stretch;
    min-height: var(--y7-sz-input);
  }
</style>
