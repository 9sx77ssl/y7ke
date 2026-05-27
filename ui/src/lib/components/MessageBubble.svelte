<script lang="ts" module>
  import type { MessageStatus } from "../types";

  export interface MessageBubbleProps {
    text: string;
    timestampMs: number;
    isMine: boolean;
    status: MessageStatus;
  }
</script>

<script lang="ts">
  // Chat message bubble. Variants per author (mine | theirs) and per status
  // (a `failed` ribbon for status===4). The subscript meta row carries a
  // timestamp + a status glyph; the glyph is suppressed for messages we
  // received (a tick on someone else's bubble is meaningless).

  import { formatTimestamp, statusBadge } from "../format";

  let { text, timestampMs, isMine, status }: MessageBubbleProps = $props();

  const badge = $derived(statusBadge(status));
  const failed = $derived(status === 4);
</script>

<article
  class="bubble"
  class:mine={isMine}
  class:theirs={!isMine}
  class:failed
>
  <div class="text" data-selectable>{text}</div>
  <div class="meta">
    <span class="ts" title="sent {new Date(timestampMs).toLocaleString()}">
      {formatTimestamp(timestampMs)}
    </span>
    {#if isMine}
      <span class="status tone-{badge.tone}" title={badge.label.toLowerCase()}>
        {badge.glyph}
      </span>
    {/if}
  </div>
</article>

<style>
  .bubble {
    max-width: min(560px, 80%);
    padding: var(--y7-sp-2) var(--y7-sp-3);
    border: 1px solid var(--y7-border-subtle);
    border-radius: var(--y7-r-md);
    background: var(--y7-bg-elevated);
    color: var(--y7-text-primary);
    font-size: var(--y7-fs-md);
    line-height: var(--y7-lh-normal);
    word-wrap: break-word;
    overflow-wrap: anywhere;
    animation: fade-in var(--y7-dur-base) var(--y7-ease);
  }
  .bubble.mine {
    align-self: flex-end;
    background: var(--y7-bg-active);
    border-color: var(--y7-border-default);
  }
  .bubble.theirs {
    align-self: flex-start;
  }
  .bubble.failed {
    border-color: var(--y7-red-dim);
    background: var(--y7-red-soft);
  }

  .text {
    white-space: pre-wrap;
  }
  .meta {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--y7-sp-2);
    margin-top: var(--y7-sp-1);
    font-size: var(--y7-fs-xs);
    color: var(--y7-text-muted);
    letter-spacing: 0.02em;
  }
  .ts {
    text-transform: lowercase;
  }
  .status {
    font-family: var(--y7-font-mono);
    letter-spacing: 0;
  }
  .status.tone-muted {
    color: var(--y7-text-muted);
  }
  .status.tone-ok {
    color: var(--y7-green);
  }
  .status.tone-warn {
    color: var(--y7-red);
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
