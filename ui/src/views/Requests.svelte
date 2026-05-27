<script lang="ts">
  import {
    acceptRequestAction,
    refreshRequests,
    rejectRequestAction,
    requests,
  } from "../lib/stores/requests.svelte";
  import { formatTimestamp, truncateY7Id } from "../lib/format";

  let busyIds = $state<Record<number, boolean>>({});
  let rowErrors = $state<Record<number, string>>({});

  $effect(() => {
    if (!requests.loadedOnce && !requests.loading) {
      void refreshRequests();
    }
  });

  async function accept(id: number): Promise<void> {
    busyIds[id] = true;
    delete rowErrors[id];
    try {
      await acceptRequestAction(id);
    } catch (err) {
      rowErrors[id] = err instanceof Error ? err.message : String(err);
    } finally {
      delete busyIds[id];
    }
  }

  async function reject(id: number): Promise<void> {
    busyIds[id] = true;
    delete rowErrors[id];
    try {
      await rejectRequestAction(id);
    } catch (err) {
      rowErrors[id] = err instanceof Error ? err.message : String(err);
    } finally {
      delete busyIds[id];
    }
  }
</script>

<section class="page">
  <header class="head">
    <h1>Requests</h1>
    <button
      type="button"
      class="refresh"
      onclick={() => {
        void refreshRequests();
      }}
      disabled={requests.loading}
      aria-label="Refresh requests"
    >
      {requests.loading ? "…" : "Refresh"}
    </button>
  </header>

  {#if requests.error}
    <p class="msg err">{requests.error}</p>
  {/if}

  <section class="group">
    <h2>Incoming <span class="count">{requests.incoming.length}</span></h2>
    {#if requests.incoming.length === 0}
      <p class="empty">No incoming requests.</p>
    {:else}
      <ul>
        {#each requests.incoming as req (req.id)}
          <li class="row">
            <div class="meta">
              <code class="y7" title={req.peer_y7_id}>
                {truncateY7Id(req.peer_y7_id)}
              </code>
              <span class="ts">{formatTimestamp(req.created_at)}</span>
            </div>
            {#if req.initial_text}
              <p class="greeting">{req.initial_text}</p>
            {/if}
            <div class="actions">
              <button
                type="button"
                disabled={busyIds[req.id]}
                onclick={() => {
                  void accept(req.id);
                }}
              >
                Accept
              </button>
              <button
                type="button"
                class="ghost"
                disabled={busyIds[req.id]}
                onclick={() => {
                  void reject(req.id);
                }}
              >
                Reject
              </button>
            </div>
            {#if rowErrors[req.id]}
              <p class="msg err small">{rowErrors[req.id]}</p>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <section class="group">
    <h2>Outgoing <span class="count">{requests.outgoing.length}</span></h2>
    {#if requests.outgoing.length === 0}
      <p class="empty">No outgoing requests pending.</p>
    {:else}
      <ul>
        {#each requests.outgoing as req (req.id)}
          <li class="row">
            <div class="meta">
              <code class="y7" title={req.peer_y7_id}>
                {truncateY7Id(req.peer_y7_id)}
              </code>
              <span class="ts">{formatTimestamp(req.created_at)}</span>
            </div>
            {#if req.initial_text}
              <p class="greeting">{req.initial_text}</p>
            {/if}
            <p class="status-line">Pending…</p>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</section>

<style>
  .page {
    height: 100%;
    overflow-y: auto;
    padding: 2rem 2.5rem;
  }
  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 1.5rem;
  }
  h1 {
    margin: 0;
    font-size: 1.25rem;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .refresh {
    font: inherit;
    font-size: 0.8rem;
    padding: 0.3rem 0.7rem;
    border-radius: 5px;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .refresh:hover:not(:disabled) {
    background: color-mix(in oklab, currentColor 8%, transparent);
  }
  .refresh:disabled {
    opacity: 0.5;
    cursor: progress;
  }
  .group {
    margin-bottom: 2rem;
  }
  h2 {
    margin: 0 0 0.75rem;
    font-size: 0.95rem;
    font-weight: 600;
    opacity: 0.75;
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  .count {
    font-size: 0.75rem;
    padding: 0.1rem 0.5rem;
    border-radius: 999px;
    background: color-mix(in oklab, currentColor 8%, transparent);
    opacity: 0.85;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .row {
    padding: 0.85rem 1rem;
    border-radius: 8px;
    border: 1px solid color-mix(in oklab, currentColor 14%, transparent);
    background: color-mix(in oklab, Canvas 100%, currentColor 2%);
  }
  .meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    margin-bottom: 0.4rem;
  }
  .y7 {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.8rem;
    padding: 0.15rem 0.4rem;
    background: color-mix(in oklab, currentColor 6%, transparent);
    border-radius: 4px;
  }
  .ts {
    font-size: 0.75rem;
    opacity: 0.55;
  }
  .greeting {
    margin: 0 0 0.6rem;
    font-size: 0.9rem;
    opacity: 0.85;
    line-height: 1.4;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .actions {
    display: flex;
    gap: 0.5rem;
  }
  button {
    font: inherit;
    padding: 0.4rem 0.85rem;
    border-radius: 5px;
    border: 1px solid color-mix(in oklab, currentColor 22%, transparent);
    background: color-mix(in oklab, currentColor 10%, transparent);
    color: inherit;
    cursor: pointer;
    font-size: 0.85rem;
  }
  button:hover:not(:disabled) {
    background: color-mix(in oklab, currentColor 16%, transparent);
  }
  button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  button.ghost {
    background: transparent;
  }
  .empty {
    margin: 0;
    font-size: 0.85rem;
    opacity: 0.55;
    padding: 0.5rem 0;
  }
  .status-line {
    margin: 0;
    font-size: 0.8rem;
    opacity: 0.6;
    font-style: italic;
  }
  .msg {
    margin: 0.5rem 0 0;
    font-size: 0.85rem;
  }
  .msg.err {
    color: color-mix(in oklab, currentColor 70%, crimson);
  }
  .msg.small {
    font-size: 0.8rem;
  }
</style>
