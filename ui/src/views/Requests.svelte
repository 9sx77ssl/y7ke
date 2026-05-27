<script lang="ts">
  // Two-section requests view. Incoming rows expose Accept (primary) + Reject
  // (ghost). Outgoing rows expose a Cancel button — that's the new V2 piece;
  // the corresponding `cancel_request` backend command is being wired in
  // parallel, so we tolerate "command not found" by toasting the error.

  import {
    acceptRequestAction,
    cancelRequestAction,
    refreshRequests,
    rejectRequestAction,
    requests,
  } from "../lib/stores/requests.svelte";
  import { formatTimestamp, truncateY7Id } from "../lib/format";
  import Button from "../lib/components/Button.svelte";
  import Card from "../lib/components/Card.svelte";
  import IconButton from "../lib/components/IconButton.svelte";
  import { toast } from "../lib/components/toast.svelte";
  import type { RequestView } from "../lib/types";

  let busyIds = $state<Record<number, boolean>>({});

  $effect(() => {
    if (!requests.loadedOnce && !requests.loading) {
      void refreshRequests();
    }
  });

  function setBusy(id: number, v: boolean): void {
    if (v) busyIds[id] = true;
    else delete busyIds[id];
  }

  async function accept(id: number): Promise<void> {
    setBusy(id, true);
    try {
      await acceptRequestAction(id);
      toast.success("contact accepted");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(id, false);
    }
  }

  async function reject(id: number): Promise<void> {
    setBusy(id, true);
    try {
      await rejectRequestAction(id);
      toast.info("request rejected");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(id, false);
    }
  }

  async function cancel(id: number): Promise<void> {
    setBusy(id, true);
    try {
      await cancelRequestAction(id);
      toast.success("request cancelled");
    } catch (err) {
      // If the backend `cancel_request` command isn't wired yet, the error
      // surfaces here instead of crashing the view.
      toast.error(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(id, false);
    }
  }
</script>

<section class="page">
  <div class="content">
    <header class="head">
      <h1>requests</h1>
      <IconButton
        size={26}
        ariaLabel="refresh requests"
        title="refresh"
        disabled={requests.loading}
        onclick={() => {
          void refreshRequests();
        }}
      >
        <svg width="14" height="14" viewBox="0 0 12 12" aria-hidden="true">
          <path
            d="M2 6a4 4 0 0 1 6.9-2.8M10 6a4 4 0 0 1-6.9 2.8"
            fill="none"
            stroke="currentColor"
            stroke-width="1.2"
            stroke-linecap="round"
          />
          <path
            d="M9 1v3H6M3 11V8h3"
            fill="none"
            stroke="currentColor"
            stroke-width="1.2"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </IconButton>
    </header>

    {#if requests.error}
      <p class="msg err">{requests.error}</p>
    {/if}

    <Card title="incoming ({requests.incoming.length})">
      {#if requests.incoming.length === 0}
        <p class="empty">no incoming requests.</p>
      {:else}
        <ul class="rows">
          {#each requests.incoming as req (req.id)}
            {@render row(req, "incoming")}
          {/each}
        </ul>
      {/if}
    </Card>

    <Card title="outgoing ({requests.outgoing.length})">
      {#if requests.outgoing.length === 0}
        <p class="empty">no outgoing requests pending.</p>
      {:else}
        <ul class="rows">
          {#each requests.outgoing as req (req.id)}
            {@render row(req, "outgoing")}
          {/each}
        </ul>
      {/if}
    </Card>
  </div>
</section>

{#snippet row(req: RequestView, kind: "incoming" | "outgoing")}
  <li class="row">
    <div class="meta">
      <code class="y7" title={req.peer_y7_id} data-selectable>
        {truncateY7Id(req.peer_y7_id)}
      </code>
      <span class="ts">{formatTimestamp(req.created_at)}</span>
    </div>
    {#if req.initial_text}
      <p class="greeting">{req.initial_text}</p>
    {/if}
    <div class="actions">
      {#if kind === "incoming"}
        <Button
          variant="primary"
          size="sm"
          disabled={busyIds[req.id]}
          title="accept this contact request"
          onclick={() => {
            void accept(req.id);
          }}
        >
          accept
        </Button>
        <Button
          variant="ghost"
          size="sm"
          disabled={busyIds[req.id]}
          title="reject this contact request"
          onclick={() => {
            void reject(req.id);
          }}
        >
          reject
        </Button>
      {:else}
        <span class="status-line">pending…</span>
        <Button
          variant="danger"
          size="sm"
          disabled={busyIds[req.id]}
          title="cancel this outgoing request"
          onclick={() => {
            void cancel(req.id);
          }}
        >
          cancel
        </Button>
      {/if}
    </div>
  </li>
{/snippet}

<style>
  .page {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--y7-sp-8) var(--y7-sp-6);
    background: var(--y7-bg-base);
  }
  .content {
    max-width: 720px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: var(--y7-sp-5);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--y7-sp-3);
  }
  h1 {
    margin: 0;
    font-size: var(--y7-fs-2xl);
    font-weight: var(--y7-fw-bold);
    color: var(--y7-text-primary);
    text-transform: lowercase;
    letter-spacing: 0.02em;
  }

  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--y7-sp-3);
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: var(--y7-sp-2);
    padding: var(--y7-sp-3);
    border: 1px solid var(--y7-border-subtle);
    border-radius: var(--y7-r-md);
    background: var(--y7-bg-base);
  }
  .meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--y7-sp-3);
  }
  .y7 {
    font-family: var(--y7-font-mono);
    font-size: var(--y7-fs-sm);
    padding: var(--y7-sp-1) var(--y7-sp-2);
    background: var(--y7-bg-elevated);
    border: 1px solid var(--y7-border-subtle);
    border-radius: var(--y7-r-sm);
    color: var(--y7-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }
  .ts {
    font-size: var(--y7-fs-xs);
    color: var(--y7-text-muted);
    text-transform: lowercase;
    letter-spacing: 0.04em;
    white-space: nowrap;
  }
  .greeting {
    margin: 0;
    padding: var(--y7-sp-2) var(--y7-sp-3);
    background: var(--y7-bg-elevated);
    border-radius: var(--y7-r-sm);
    font-size: var(--y7-fs-md);
    color: var(--y7-text-primary);
    line-height: var(--y7-lh-normal);
    white-space: pre-wrap;
    word-break: break-word;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--y7-sp-2);
    flex-wrap: wrap;
  }
  .status-line {
    flex: 1;
    font-size: var(--y7-fs-sm);
    color: var(--y7-text-muted);
    text-transform: lowercase;
    letter-spacing: 0.02em;
  }
  .empty {
    margin: 0;
    font-size: var(--y7-fs-md);
    color: var(--y7-text-muted);
    text-transform: lowercase;
  }
  .msg {
    margin: 0;
    font-size: var(--y7-fs-sm);
  }
  .msg.err {
    color: var(--y7-red);
  }
</style>
