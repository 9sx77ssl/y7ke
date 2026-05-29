<script lang="ts">
  // Connectivity debug pane — V2-A4 + V2-A5 visibility.
  //
  // Polls three diagnostics every 3s while mounted: per-peer
  // `list_active_connections`, the aggregate AutoNAT verdict, and the
  // DCUtR upgrade counters. Subscribes to `nat_status_changed` /
  // `presence_changed` events for instant refresh on the rare verdict
  // flip / connection event so the user doesn't see stale data.
  //
  // Strictly minimal-monochrome. No charts, no animations beyond the
  // existing transitions. Text + small badges only.

  import {
    getDcutrStats,
    getNatStatus,
    listActiveConnections,
    listBootstraps,
  } from "../lib/bridge";
  import type { ConnectionView } from "../lib/gen/ConnectionView";
  import type { DcutrStats } from "../lib/gen/DcutrStats";
  import type { NatReachability } from "../lib/gen/NatReachability";
  import type { BootstrapEntry } from "../lib/types-settings-stub";
  import { eventState } from "../lib/stores/events.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import Card from "../lib/components/Card.svelte";
  import { log } from "../lib/log";

  const logger = log("Connectivity");

  let connections = $state<ConnectionView[]>([]);
  let bootstraps = $state<BootstrapEntry[]>([]);
  let nat = $state<NatReachability>("unknown");
  let dcutr = $state<DcutrStats>({
    attempts: 0,
    successes: 0,
    failures: 0,
  });

  // Monotonic generation: during a reconnect storm the poller + the event
  // reactor can launch overlapping refreshes; only the newest one is allowed
  // to commit, so a slow older call can't clobber fresh state with stale data.
  let gen = 0;

  // Relay fallback is "active" when a peer is reachable only via relay.
  const relayActive = $derived(
    connections.some((c) => c.kind === "relayed") &&
      !connections.some((c) => c.kind === "direct"),
  );

  async function refresh(): Promise<void> {
    const my = ++gen;
    try {
      const [c, b, n, d] = await Promise.all([
        listActiveConnections(),
        listBootstraps(),
        getNatStatus(),
        getDcutrStats(),
      ]);
      if (my !== gen) return; // superseded by a newer refresh
      connections = c;
      bootstraps = b;
      nat = n;
      dcutr = d;
    } catch (err) {
      logger.warn("refresh failed", err instanceof Error ? err.message : err);
    }
  }

  // Initial fetch + 3s polling while mounted.
  $effect(() => {
    void refresh();
    const id = setInterval(() => {
      void refresh();
    }, 3000);
    return () => clearInterval(id);
  });

  // React to AppEvents that demand a refresh, debounced so a presence/NAT
  // storm coalesces into a single fetch (~150ms quiet) instead of one
  // 4-RPC burst per event. Skip the first run — the poller above already
  // does the mount fetch, so this shouldn't double-fire on open.
  let reactorPrimed = false;
  $effect(() => {
    // Touching these dependencies forces re-run when the events fire.
    const _ = eventState.presenceRev + eventState.natRev;
    void _;
    if (!reactorPrimed) {
      reactorPrimed = true;
      return;
    }
    const id = setTimeout(() => {
      void refresh();
    }, 150);
    return () => clearTimeout(id);
  });

  function dcutrRatePct(): number | null {
    const n = Number(dcutr.attempts);
    const s = Number(dcutr.successes);
    if (n === 0) return null;
    return Math.round((s / n) * 100);
  }

  function natTone(v: NatReachability): "ok" | "warn" | "muted" {
    switch (v) {
      case "public":
        return "ok";
      case "private":
        return "warn";
      case "unknown":
        return "muted";
    }
  }

  function natLabel(v: NatReachability): string {
    return v.toUpperCase();
  }

  function kindLabel(k: ConnectionView["kind"]): string {
    return k.toUpperCase();
  }

  function kindTone(k: ConnectionView["kind"]): "ok" | "warn" | "muted" | "info" {
    switch (k) {
      case "direct":
        return "ok";
      case "lan":
      case "internet":
        return "info";
      case "relayed":
        return "warn";
      default:
        return "muted";
    }
  }

  function transportLabel(t: ConnectionView["transport"]): string {
    if (t === null) return "—";
    return t.toUpperCase();
  }

  function truncateY7(y7: string): string {
    if (y7.length <= 18) return y7;
    return y7.slice(0, 12) + "…" + y7.slice(-4);
  }

  function bootstrapHost(b: BootstrapEntry): string {
    // Pull the host segment from /dns4/HOST/... or /ip4/HOST/...
    const parts = b.multiaddr.split("/");
    const idx = parts.findIndex(
      (p) => p === "dns4" || p === "dns6" || p === "ip4" || p === "ip6",
    );
    if (idx === -1 || idx + 1 >= parts.length) return b.multiaddr;
    return parts[idx + 1] ?? b.multiaddr;
  }

  // Match the Settings pill labels: Internet is shown as "Y7net".
  function dialModeLabel(m: string | undefined): string {
    if (m === "LanOnly") return "lan only";
    if (m === "Internet") return "Y7net";
    return "—";
  }

  function reachLabel(b: BootstrapEntry): string {
    if (b.last_ping_failed) return "unreachable";
    if (b.last_ping_ms !== null) return "reachable";
    return "—";
  }
  function reachTone(b: BootstrapEntry): "ok" | "fail" | "muted" {
    if (b.last_ping_failed) return "fail";
    if (b.last_ping_ms !== null) return "ok";
    return "muted";
  }
</script>

<section class="page">
  <div class="content">
    <header class="head">
      <h1>connectivity</h1>
      <p class="sub">how this device is currently reaching everyone.</p>
    </header>

    <!-- ── system panel ────────────────────────────────────────────────── -->
    <Card title="system">
      <div class="metrics">
        <div class="metric">
          <span class="label">dial mode</span>
          <span class="value">{dialModeLabel(settingsStore.settings?.dial_mode)}</span>
        </div>
        <div class="metric">
          <span class="label">nat status</span>
          <span class="pill tone-{natTone(nat)}">{natLabel(nat)}</span>
        </div>
        <div class="metric">
          <span class="label">relay fallback</span>
          <span class="pill tone-{relayActive ? 'warn' : 'muted'}">
            {relayActive ? "active" : "no"}
          </span>
        </div>
        <div class="metric">
          <span class="label">dcutr</span>
          <span class="value">
            {Number(dcutr.successes)} / {Number(dcutr.attempts)}
            {#if dcutrRatePct() !== null}
              ({dcutrRatePct()}%)
            {/if}
          </span>
        </div>
      </div>
    </Card>

    <!-- ── bootstraps ─────────────────────────────────────────────────── -->
    <Card title="bootstraps">
      {#if bootstraps.length === 0}
        <p class="empty">no bootstraps configured.</p>
      {:else}
        <ul class="rows">
          {#each bootstraps as b (b.multiaddr)}
            <li class="row">
              <span class="addr">{bootstrapHost(b)}</span>
              {#if b.is_default}
                <span class="pill tone-muted">default</span>
              {/if}
              <span class="ping tone-{reachTone(b)}">{reachLabel(b)}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </Card>

    <!-- ── active connections ─────────────────────────────────────────── -->
    <Card title="active connections">
      {#if connections.length === 0}
        <p class="empty">nothing connected right now.</p>
      {:else}
        <ul class="rows">
          {#each connections as c (c.y7_id)}
            <li class="row">
              <span class="y7" title={c.y7_id}>{truncateY7(c.y7_id)}</span>
              <span class="pill tone-{kindTone(c.kind)}">
                {kindLabel(c.kind)}
              </span>
              <span class="pill tone-muted">{transportLabel(c.transport)}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </Card>
  </div>
</section>

<style>
  .page {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--y7-sp-6) var(--y7-sp-6);
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
    flex-direction: column;
    gap: var(--y7-sp-1);
  }
  h1 {
    margin: 0;
    font-size: var(--y7-fs-2xl);
    font-weight: var(--y7-fw-bold);
    color: var(--y7-text-primary);
    text-transform: lowercase;
    letter-spacing: 0.02em;
  }
  .sub {
    margin: 0;
    font-size: var(--y7-fs-md);
    color: var(--y7-text-secondary);
    text-transform: lowercase;
  }

  .metrics {
    display: flex;
    flex-wrap: wrap;
    gap: var(--y7-sp-4) var(--y7-sp-6);
  }
  .metric {
    display: flex;
    flex-direction: column;
    gap: var(--y7-sp-1);
    min-width: 90px;
  }
  .metric .label {
    font-size: var(--y7-fs-xs);
    color: var(--y7-text-muted);
    text-transform: lowercase;
    letter-spacing: 0.06em;
  }
  .metric .value {
    font-family: var(--y7-font-mono);
    font-size: var(--y7-fs-md);
    color: var(--y7-text-primary);
    text-transform: lowercase;
  }

  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--y7-sp-2);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--y7-sp-2);
    flex-wrap: wrap;
  }
  .y7,
  .addr {
    font-family: var(--y7-font-mono);
    font-size: var(--y7-fs-sm);
    color: var(--y7-text-primary);
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pill,
  .ping {
    font-family: var(--y7-font-mono);
    font-size: var(--y7-fs-xs);
    padding: 2px var(--y7-sp-2);
    border-radius: var(--y7-r-full);
    border: 1px solid transparent;
    text-transform: lowercase;
    letter-spacing: 0.06em;
    white-space: nowrap;
  }
  .tone-ok {
    color: var(--y7-green);
    background: var(--y7-green-soft);
    border-color: rgba(74, 222, 128, 0.3);
  }
  .tone-warn {
    color: var(--y7-warn);
    background: var(--y7-warn-soft);
    border-color: var(--y7-warn-border);
  }
  .tone-fail {
    color: var(--y7-red);
    background: var(--y7-red-soft);
    border-color: rgba(239, 68, 68, 0.3);
  }
  .tone-muted {
    /* Legible-but-subtle: text-muted on a near-invisible border read as
     * ~2:1 on the base bg (the 'tcp'/'default' pills looked washed out).
     * Secondary text + default border keeps it monochrome yet readable. */
    color: var(--y7-text-secondary);
    background: transparent;
    border-color: var(--y7-border-default);
  }
  .tone-info {
    color: var(--y7-blue);
    background: var(--y7-blue-soft);
    border-color: rgba(96, 165, 250, 0.3);
  }

  .empty {
    margin: 0;
    font-size: var(--y7-fs-md);
    color: var(--y7-text-muted);
    text-transform: lowercase;
  }
</style>
