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
  import { formatTimestamp, statusLabel } from "../format";

  let { text, timestampMs, isMine, status }: MessageBubbleProps = $props();

  const label = $derived(statusLabel(status));
</script>

<article
  class="bubble"
  class:mine={isMine}
  class:theirs={!isMine}
>
  <div class="text">{text}</div>
  <div class="meta">
    <span class="ts" title={new Date(timestampMs).toLocaleString()}>
      {formatTimestamp(timestampMs)}
    </span>
    {#if isMine}
      <span class="status" class:failed title={label}>
        {#if status === 0}
          <!-- clock: sending -->
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
            <circle cx="7" cy="7" r="5.5"/>
            <path d="M7 4v3.5L9.5 9"/>
          </svg>
        {:else if status === 1}
          <!-- single check: sent -->
          <svg xmlns="http://www.w3.org/2000/svg" width="13" height="12" viewBox="0 0 13 12" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
            <path d="M1.5 6L5.5 10L11.5 2"/>
          </svg>
        {:else if status === 2 || status === 3}
          <!-- double check: delivered / synced -->
          <svg xmlns="http://www.w3.org/2000/svg" width="17" height="12" viewBox="0 0 17 12" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
            <path d="M1 6L5 10L11 2"/>
            <path d="M6 6L10 10L16 2"/>
          </svg>
        {:else if status === 4}
          <!-- circle exclamation: failed -->
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
            <circle cx="7" cy="7" r="5.5"/>
            <path d="M7 4.5v3.5"/>
            <circle cx="7" cy="10.5" r="0.6" fill="currentColor" stroke="none"/>
          </svg>
        {/if}
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
  .text {
    white-space: pre-wrap;
    user-select: text;
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
    user-select: none;
  }
  .ts {
    text-transform: lowercase;
  }
  .status {
    display: flex;
    align-items: center;
    color: var(--y7-text-muted);
    line-height: 1;
  }
  .status.failed {
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
