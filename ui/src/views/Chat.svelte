<script lang="ts">
  import {
    chat,
    openConversation,
    sendText,
  } from "../lib/stores/chat.svelte";
  import { findContact } from "../lib/stores/contacts.svelte";
  import { getPresence, presenceLabel } from "../lib/stores/presence.svelte";
  import {
    formatTimestamp,
    statusBadge,
    truncateY7Id,
  } from "../lib/format";
  import type { MessageView } from "../lib/types";

  interface Props {
    peerY7Id: string;
  }

  let { peerY7Id }: Props = $props();

  let composer = $state("");
  let scrollEl = $state<HTMLDivElement | undefined>(undefined);
  let lastSeenCount = 0;

  // Re-open the conversation when the peer changes (sidebar click switches it).
  $effect(() => {
    if (chat.peerY7Id !== peerY7Id) {
      void openConversation(peerY7Id);
    }
  });

  // Auto-scroll to bottom whenever a new message appears or we just opened.
  $effect(() => {
    const count = chat.messages.length;
    if (!scrollEl) return;
    if (count !== lastSeenCount) {
      lastSeenCount = count;
      // Defer to let layout settle (Svelte 5 will have flushed the DOM by here
      // but we still want the post-paint scrollHeight).
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

  async function submit(): Promise<void> {
    const text = composer.trim();
    if (text.length === 0 || chat.sending) return;
    composer = "";
    await sendText(text);
  }

  function handleKeydown(ev: KeyboardEvent): void {
    if (ev.key === "Enter" && !ev.shiftKey) {
      ev.preventDefault();
      void submit();
    }
  }

  function presenceTone(): string {
    switch (presence) {
      case "lan":
        return "ok";
      case "connecting":
        return "warn";
      case "offline":
        return "muted";
    }
  }

  function bubbleClasses(msg: MessageView): string {
    const classes = ["bubble"];
    if (msg.is_mine) classes.push("mine");
    else classes.push("theirs");
    if (msg.status === 4) classes.push("failed");
    return classes.join(" ");
  }
</script>

<section class="chat">
  <header class="head">
    <div class="who">
      <span class="name">{displayName}</span>
      <code class="y7" title={peerY7Id}>{truncateY7Id(peerY7Id)}</code>
    </div>
    <span class="presence" data-tone={presenceTone()}>
      <span class="dot" aria-hidden="true"></span>
      {presenceLabel(presence)}
    </span>
  </header>

  <div class="scroll" bind:this={scrollEl}>
    {#if chat.loading && chat.messages.length === 0}
      <p class="info">Loading…</p>
    {/if}

    {#if chat.error}
      <p class="info err">{chat.error}</p>
    {/if}

    {#if !chat.loading && chat.messages.length === 0 && !chat.error}
      <p class="info">
        No messages yet. Say something — it'll deliver as soon as
        {displayName} is on the LAN.
      </p>
    {/if}

    <ol class="list">
      {#each chat.messages as msg (msg.message_id)}
        {@const badge = statusBadge(msg.status)}
        <li class={bubbleClasses(msg)}>
          <div class="text">{msg.text}</div>
          <div class="meta">
            <span class="ts">{formatTimestamp(msg.timestamp_ms)}</span>
            {#if msg.is_mine}
              <span class="status" data-tone={badge.tone} title={badge.label}>
                {badge.glyph}
              </span>
            {/if}
          </div>
        </li>
      {/each}
    </ol>
  </div>

  <form
    class="composer"
    onsubmit={(ev) => {
      ev.preventDefault();
      void submit();
    }}
  >
    <textarea
      bind:value={composer}
      placeholder="Write a message…"
      rows="2"
      onkeydown={handleKeydown}
      disabled={chat.sending}
      aria-label="Message"
    ></textarea>
    <button type="submit" disabled={chat.sending || composer.trim().length === 0}>
      {chat.sending ? "Sending…" : "Send"}
    </button>
  </form>
</section>

<style>
  .chat {
    height: 100%;
    display: grid;
    grid-template-rows: auto 1fr auto;
    min-height: 0;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.85rem 1.25rem;
    border-bottom: 1px solid color-mix(in oklab, currentColor 12%, transparent);
    background: color-mix(in oklab, Canvas 100%, currentColor 1.5%);
  }
  .who {
    display: flex;
    align-items: baseline;
    gap: 0.55rem;
    min-width: 0;
    overflow: hidden;
  }
  .name {
    font-weight: 600;
    font-size: 0.95rem;
    letter-spacing: -0.01em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .y7 {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.75rem;
    padding: 0.1rem 0.4rem;
    background: color-mix(in oklab, currentColor 6%, transparent);
    border-radius: 4px;
    opacity: 0.7;
    white-space: nowrap;
  }
  .presence {
    font-size: 0.78rem;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    opacity: 0.8;
    white-space: nowrap;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: currentColor;
    opacity: 0.5;
  }
  .presence[data-tone="ok"] {
    color: color-mix(in oklab, currentColor 55%, seagreen);
  }
  .presence[data-tone="warn"] {
    color: color-mix(in oklab, currentColor 55%, goldenrod);
  }
  .presence[data-tone="muted"] {
    opacity: 0.55;
  }

  .scroll {
    overflow-y: auto;
    padding: 1rem 1.25rem 1.25rem;
    min-height: 0;
  }
  .info {
    margin: 1rem 0;
    text-align: center;
    font-size: 0.85rem;
    opacity: 0.6;
  }
  .info.err {
    color: color-mix(in oklab, currentColor 70%, crimson);
    opacity: 0.85;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .bubble {
    max-width: min(36rem, 80%);
    padding: 0.55rem 0.8rem;
    border-radius: 14px;
    background: color-mix(in oklab, currentColor 6%, transparent);
    line-height: 1.4;
    font-size: 0.9rem;
    word-wrap: break-word;
    animation: fade-in 130ms ease-out;
  }
  .bubble.mine {
    align-self: flex-end;
    background: color-mix(in oklab, AccentColor 24%, transparent);
    border-bottom-right-radius: 4px;
  }
  .bubble.theirs {
    align-self: flex-start;
    border-bottom-left-radius: 4px;
  }
  .bubble.failed {
    background: color-mix(in oklab, crimson 18%, transparent);
  }
  .text {
    white-space: pre-wrap;
  }
  .meta {
    display: flex;
    justify-content: flex-end;
    gap: 0.4rem;
    margin-top: 0.25rem;
    font-size: 0.7rem;
    opacity: 0.6;
  }
  .status[data-tone="ok"] {
    color: color-mix(in oklab, currentColor 50%, seagreen);
    opacity: 0.9;
  }
  .status[data-tone="warn"] {
    color: color-mix(in oklab, currentColor 60%, crimson);
    opacity: 1;
  }

  .composer {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0.5rem;
    padding: 0.75rem 1.25rem 1rem;
    border-top: 1px solid color-mix(in oklab, currentColor 12%, transparent);
    background: color-mix(in oklab, Canvas 100%, currentColor 1.5%);
  }
  textarea {
    font: inherit;
    font-size: 0.9rem;
    padding: 0.55rem 0.75rem;
    border-radius: 8px;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    background: Canvas;
    color: inherit;
    resize: vertical;
    min-height: 2.4rem;
    max-height: 12rem;
  }
  textarea:disabled {
    opacity: 0.6;
  }
  button {
    font: inherit;
    padding: 0 1.1rem;
    border-radius: 8px;
    border: 1px solid color-mix(in oklab, currentColor 22%, transparent);
    background: color-mix(in oklab, AccentColor 26%, transparent);
    color: inherit;
    cursor: pointer;
    font-size: 0.9rem;
    align-self: end;
    height: 2.4rem;
  }
  button:hover:not(:disabled) {
    background: color-mix(in oklab, AccentColor 36%, transparent);
  }
  button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  @keyframes fade-in {
    from {
      opacity: 0;
      transform: translateY(2px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
