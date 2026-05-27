<script lang="ts">
  // Settings view — dial-mode toggles + bootstrap-node editor.
  //
  // The "extra" bootstraps list comes from `settings.extra_bootstraps` and
  // edits are committed only on Save. The hardcoded default bootstrap is
  // returned by `list_bootstraps` with `is_default = true` — that row is
  // rendered locked (readonly input, no delete button).

  import {
    pingAll,
    refreshSettings,
    saveSettings,
    settingsStore,
  } from "../lib/stores/settings.svelte";
  import type { BootstrapEntry, DialModes } from "../lib/types-settings-stub";
  import Button from "../lib/components/Button.svelte";
  import Card from "../lib/components/Card.svelte";
  import Toggle from "../lib/components/Toggle.svelte";
  import { toast } from "../lib/components/toast.svelte";
  import { log } from "../lib/log";

  const logger = log("Settings");

  // Local editable copy — committed on save.
  let dialModes = $state<DialModes>({
    lan: true,
    internet: true,
    relay: true,
    p2p: false,
  });

  // Local editable rows; mirrors `settingsStore.bootstraps` plus optional
  // empty/in-progress rows the user has added with the "+ add" button.
  interface Row {
    multiaddr: string;
    is_default: boolean;
    last_ping_ms: number | null;
    last_ping_failed: boolean;
  }
  let rows = $state<Row[]>([]);

  // Track loaded state so we don't clobber edits on every refresh.
  let hydrated = $state(false);

  $effect(() => {
    if (!settingsStore.loadedOnce && !settingsStore.loading) {
      void refreshSettings();
    }
  });

  // Hydrate local copy once when store first loads.
  $effect(() => {
    if (
      !hydrated &&
      settingsStore.loadedOnce &&
      settingsStore.settings !== null
    ) {
      dialModes = { ...settingsStore.settings.dial_modes };
      rows = settingsStore.bootstraps.map((b) => ({ ...b }));
      hydrated = true;
      logger.debug("hydrated from store");
    }
  });

  // When ping_all returns, merge the new ping values into existing rows that
  // match by multiaddr (we don't blow away user-typed-but-not-saved rows).
  function mergePingResults(updated: BootstrapEntry[]): void {
    const byAddr = new Map(updated.map((u) => [u.multiaddr, u] as const));
    rows = rows.map((r) => {
      const u = byAddr.get(r.multiaddr);
      if (u === undefined) return r;
      return {
        ...r,
        last_ping_ms: u.last_ping_ms,
        last_ping_failed: u.last_ping_failed,
      };
    });
  }

  // ── derived: active strategy label ────────────────────────────────────────
  const strategyLabel = $derived(deriveStrategy(dialModes));

  function deriveStrategy(m: DialModes): string {
    const { lan, internet, relay, p2p } = m;
    const sel = [
      lan && "lan",
      internet && "internet",
      relay && "relay",
      p2p && "p2p",
    ].filter(Boolean) as string[];
    if (sel.length === 0) return "nothing selected — you won't connect to anyone";
    if (lan && !internet && !relay && !p2p)
      return "lan only — peers must be on the same wifi";
    if (lan && internet && !relay && !p2p)
      return "direct only — works if both peers are publicly reachable";
    if (!lan && !internet && relay && !p2p)
      return "relay-only — slower but works through any nat";
    if (lan && internet && relay && !p2p)
      return "auto — tries direct first, falls back to relay";
    if (!lan && !internet && !relay && p2p)
      return "p2p only — not yet implemented (placeholder for v2-a5)";
    if (p2p && relay && !lan && !internet)
      return "p2p preferred, relay fallback (p2p coming soon)";
    if (lan && relay && !internet && !p2p)
      return "lan + relay — same-wifi peers direct, others through relay";
    if (internet && relay && !lan && !p2p)
      return "internet + relay — direct on open networks, relay fallback";
    return `custom mix (${sel.join(" + ")})`;
  }

  // ── derived: dirty / save-disabled ────────────────────────────────────────
  const hasModeSelected = $derived(
    dialModes.lan || dialModes.internet || dialModes.relay || dialModes.p2p,
  );

  const allMultiaddrsValid = $derived(
    rows.every((r) => r.is_default || r.multiaddr.trim().length === 0 || isLikelyMultiaddr(r.multiaddr)),
  );

  const dirty = $derived(
    hydrated &&
      settingsStore.settings !== null &&
      (modesEqual(dialModes, settingsStore.settings.dial_modes) === false ||
        extraBootstrapsDirty()),
  );

  function modesEqual(a: DialModes, b: DialModes): boolean {
    return (
      a.lan === b.lan &&
      a.internet === b.internet &&
      a.relay === b.relay &&
      a.p2p === b.p2p
    );
  }

  function extraBootstrapsDirty(): boolean {
    if (settingsStore.settings === null) return false;
    const live = rows
      .filter((r) => !r.is_default)
      .map((r) => r.multiaddr.trim())
      .filter((s) => s.length > 0);
    const saved = settingsStore.settings.extra_bootstraps;
    if (live.length !== saved.length) return true;
    for (let i = 0; i < live.length; i++) {
      if (live[i] !== saved[i]) return true;
    }
    return false;
  }

  // Loose regex — matches the visible structure. Backend does real parsing.
  const MULTIADDR_RE =
    /^\/(dns4|dns6|ip4|ip6)\/[^/]+\/tcp\/\d+\/p2p\/12D3KooW[a-zA-Z0-9]+$/;

  function isLikelyMultiaddr(s: string): boolean {
    return MULTIADDR_RE.test(s.trim());
  }

  // ── handlers ──────────────────────────────────────────────────────────────
  function toggleMode(key: keyof DialModes, next: boolean): void {
    dialModes = { ...dialModes, [key]: next };
    logger.debug("toggle", key, "->", String(next));
  }

  function addRow(): void {
    rows = [
      ...rows,
      {
        multiaddr: "",
        is_default: false,
        last_ping_ms: null,
        last_ping_failed: false,
      },
    ];
    logger.debug("added empty bootstrap row");
  }

  function removeRow(index: number): void {
    const target = rows[index];
    if (target === undefined || target.is_default) return;
    rows = rows.filter((_, i) => i !== index);
    logger.debug("removed bootstrap row", String(index));
  }

  function updateRowAddr(index: number, value: string): void {
    rows = rows.map((r, i) => (i === index ? { ...r, multiaddr: value } : r));
  }

  async function onSave(): Promise<void> {
    if (!hasModeSelected) {
      toast.error("select at least one connection mode");
      return;
    }
    if (!allMultiaddrsValid) {
      toast.error("one or more bootstrap multiaddrs look malformed");
      return;
    }
    const extras = rows
      .filter((r) => !r.is_default)
      .map((r) => r.multiaddr.trim())
      .filter((s) => s.length > 0);
    logger.info("saving", `dial_modes=${JSON.stringify(dialModes)}`, `extras=${extras.length}`);
    try {
      await saveSettings({ ...dialModes }, extras);
      toast.success("settings saved");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      logger.error("save failed", msg);
      toast.error(`save failed: ${msg}`);
    }
  }

  async function onPingAll(): Promise<void> {
    logger.info("pinging all bootstraps");
    try {
      const updated = await pingAll();
      mergePingResults(updated);
      // Pick the fastest successful ping for the toast summary.
      const okEntries = updated.filter(
        (b) => !b.last_ping_failed && b.last_ping_ms !== null,
      );
      okEntries.sort(
        (a, b) => (a.last_ping_ms ?? Infinity) - (b.last_ping_ms ?? Infinity),
      );
      const best = okEntries[0];
      if (best !== undefined && best.last_ping_ms !== null) {
        const host = extractHost(best.multiaddr) ?? best.multiaddr;
        toast.success(`fastest: ${host} at ${best.last_ping_ms} ms`);
      } else {
        toast.info("no bootstrap responded");
      }
      logger.info("ping result", `ok=${okEntries.length}/${updated.length}`);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      logger.error("ping failed", msg);
      toast.error(`ping failed: ${msg}`);
    }
  }

  function extractHost(multiaddr: string): string | null {
    // /dns4/<host>/tcp/<port>/p2p/<peer-id>
    const parts = multiaddr.split("/");
    // [ "", "dns4", "host", "tcp", "port", "p2p", "peer" ]
    const idx = parts.findIndex(
      (p) => p === "dns4" || p === "dns6" || p === "ip4" || p === "ip6",
    );
    if (idx === -1 || idx + 1 >= parts.length) return null;
    return parts[idx + 1] ?? null;
  }

  function latencyClass(r: Row): "ok" | "warn" | "fail" | "muted" {
    if (r.last_ping_failed) return "fail";
    if (r.last_ping_ms === null) return "muted";
    if (r.last_ping_ms <= 150) return "ok";
    return "warn";
  }

  function latencyText(r: Row): string {
    if (r.last_ping_failed) return "failed";
    if (r.last_ping_ms === null) return "—";
    return `${r.last_ping_ms} ms`;
  }
</script>

<section class="page">
  <div class="content">
    <header class="head">
      <h1>settings</h1>
      <p class="sub">how this device finds and talks to other peers.</p>
    </header>

    {#if settingsStore.error}
      <p class="msg err">{settingsStore.error}</p>
    {/if}

    <!-- ── connection modes ───────────────────────────────────────────── -->
    <Card title="connection modes">
      <div class="chips">
        <Toggle
          label="lan"
          checked={dialModes.lan}
          onchange={(v) => toggleMode("lan", v)}
        />
        <Toggle
          label="internet"
          checked={dialModes.internet}
          onchange={(v) => toggleMode("internet", v)}
        />
        <Toggle
          label="relay"
          checked={dialModes.relay}
          onchange={(v) => toggleMode("relay", v)}
        />
        <Toggle
          label="p2p"
          checked={dialModes.p2p}
          onchange={(v) => toggleMode("p2p", v)}
        />
      </div>
      <p class="hint">
        at least one must be selected. relay routes through bootstrap servers;
        p2p is an experimental hole-punching mode that is not yet implemented
        (it's a placeholder for v2-a5).
      </p>
      <p class="strategy">
        <span class="strategy-label">active strategy:</span>
        <span class="strategy-text">{strategyLabel}</span>
      </p>
    </Card>

    <!-- ── bootstrap nodes ────────────────────────────────────────────── -->
    <Card title="bootstrap nodes">
      {#if rows.length === 0}
        <p class="empty">no bootstrap entries.</p>
      {:else}
        <ul class="rows">
          {#each rows as r, i (i)}
            {@const invalid =
              !r.is_default &&
              r.multiaddr.trim().length > 0 &&
              !isLikelyMultiaddr(r.multiaddr)}
            <li class="row">
              <div class="addr-wrap">
                <input
                  type="text"
                  class="input"
                  class:locked={r.is_default}
                  class:invalid
                  value={r.multiaddr}
                  readonly={r.is_default}
                  placeholder="/dns4/host/tcp/4101/p2p/12D3KooW…"
                  spellcheck="false"
                  aria-label="bootstrap multiaddr"
                  oninput={(e) =>
                    updateRowAddr(i, (e.currentTarget as HTMLInputElement).value)}
                />
                {#if r.is_default}
                  <span class="badge default" title="hardcoded fallback — cannot be edited or removed">
                    default
                  </span>
                {/if}
              </div>

              <span class="latency tone-{latencyClass(r)}" aria-label="latency">
                {latencyText(r)}
              </span>

              {#if !r.is_default}
                <button
                  type="button"
                  class="x"
                  title="remove bootstrap"
                  aria-label="remove bootstrap"
                  onclick={() => removeRow(i)}
                >
                  ×
                </button>
              {:else}
                <span class="x-spacer" aria-hidden="true"></span>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      <button type="button" class="add-btn" onclick={addRow}>
        + add bootstrap
      </button>
    </Card>

    <!-- ── actions bar ─────────────────────────────────────────────────── -->
    <div class="action-bar">
      <Button
        variant="primary"
        disabled={settingsStore.saving ||
          !hasModeSelected ||
          !allMultiaddrsValid ||
          !dirty}
        title="save settings"
        onclick={() => {
          void onSave();
        }}
      >
        {settingsStore.saving ? "saving…" : "save settings"}
      </Button>
      <Button
        variant="ghost"
        disabled={settingsStore.pinging || rows.length === 0}
        title="ping every bootstrap and measure latency"
        onclick={() => {
          void onPingAll();
        }}
      >
        {settingsStore.pinging ? "pinging…" : "ping all"}
      </Button>
    </div>
  </div>
</section>

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

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--y7-sp-2);
  }
  .hint {
    margin: var(--y7-sp-3) 0 0;
    font-size: var(--y7-fs-sm);
    color: var(--y7-text-muted);
    line-height: var(--y7-lh-relaxed);
    text-transform: lowercase;
  }
  .strategy {
    margin: var(--y7-sp-3) 0 0;
    padding: var(--y7-sp-2) var(--y7-sp-3);
    background: var(--y7-bg-base);
    border: 1px solid var(--y7-border-subtle);
    border-radius: var(--y7-r-sm);
    font-size: var(--y7-fs-sm);
    text-transform: lowercase;
    display: flex;
    gap: var(--y7-sp-2);
    align-items: baseline;
    flex-wrap: wrap;
  }
  .strategy-label {
    color: var(--y7-text-muted);
    letter-spacing: 0.04em;
  }
  .strategy-text {
    color: var(--y7-text-primary);
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
    display: grid;
    grid-template-columns: 1fr auto auto;
    gap: var(--y7-sp-2);
    align-items: center;
  }
  .addr-wrap {
    position: relative;
    display: flex;
    align-items: center;
    min-width: 0;
  }
  .input {
    width: 100%;
    height: var(--y7-sz-input);
    padding: 0 var(--y7-sp-3);
    background: var(--y7-bg-base);
    border: 1px solid var(--y7-border-default);
    border-radius: var(--y7-r-md);
    color: var(--y7-text-primary);
    font-family: var(--y7-font-mono);
    font-size: var(--y7-fs-sm);
    line-height: 1;
    transition: border-color var(--y7-dur-fast) var(--y7-ease);
  }
  .input::placeholder {
    color: var(--y7-text-muted);
  }
  .input:focus {
    outline: none;
    border-color: var(--y7-border-focus);
  }
  .input.locked {
    color: var(--y7-text-secondary);
    background: var(--y7-bg-elevated);
    cursor: default;
    padding-right: 76px;
  }
  .input.invalid {
    border-color: var(--y7-red-dim);
  }
  .badge {
    position: absolute;
    right: var(--y7-sp-2);
    top: 50%;
    transform: translateY(-50%);
    padding: 2px var(--y7-sp-2);
    font-size: var(--y7-fs-xs);
    font-family: var(--y7-font-mono);
    text-transform: lowercase;
    letter-spacing: 0.06em;
    background: var(--y7-bg-active);
    color: var(--y7-text-secondary);
    border: 1px solid var(--y7-border-subtle);
    border-radius: var(--y7-r-sm);
    pointer-events: none;
  }
  .latency {
    font-family: var(--y7-font-mono);
    font-size: var(--y7-fs-xs);
    padding: 3px var(--y7-sp-2);
    border-radius: var(--y7-r-full);
    min-width: 52px;
    text-align: center;
    border: 1px solid transparent;
    text-transform: lowercase;
    letter-spacing: 0.04em;
    white-space: nowrap;
  }
  .latency.tone-ok {
    color: var(--y7-green);
    background: var(--y7-green-soft);
    border-color: rgba(74, 222, 128, 0.3);
  }
  .latency.tone-warn {
    color: var(--y7-warn);
    background: var(--y7-warn-soft);
    border-color: rgba(245, 200, 50, 0.3);
  }
  .latency.tone-fail {
    color: var(--y7-red);
    background: var(--y7-red-soft);
    border-color: rgba(239, 68, 68, 0.3);
  }
  .latency.tone-muted {
    color: var(--y7-text-muted);
    background: transparent;
    border-color: var(--y7-border-subtle);
  }

  .x {
    width: var(--y7-sz-btn-md);
    height: var(--y7-sz-btn-md);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: 1px solid var(--y7-border-default);
    border-radius: var(--y7-r-md);
    color: var(--y7-text-secondary);
    font-family: var(--y7-font-mono);
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
    transition:
      background-color var(--y7-dur-fast) var(--y7-ease),
      color var(--y7-dur-fast) var(--y7-ease),
      border-color var(--y7-dur-fast) var(--y7-ease);
  }
  .x:hover {
    background: var(--y7-red-soft);
    border-color: var(--y7-red-dim);
    color: var(--y7-red);
  }
  .x-spacer {
    width: var(--y7-sz-btn-md);
    height: var(--y7-sz-btn-md);
  }

  .add-btn {
    margin-top: var(--y7-sp-3);
    width: 100%;
    height: var(--y7-sz-btn-md);
    background: transparent;
    border: 1px dashed var(--y7-border-default);
    border-radius: var(--y7-r-md);
    color: var(--y7-text-muted);
    font-family: var(--y7-font-mono);
    font-size: var(--y7-fs-md);
    text-transform: lowercase;
    letter-spacing: 0.04em;
    cursor: pointer;
    transition:
      border-color var(--y7-dur-fast) var(--y7-ease),
      color var(--y7-dur-fast) var(--y7-ease),
      background-color var(--y7-dur-fast) var(--y7-ease);
  }
  .add-btn:hover {
    background: var(--y7-bg-hover);
    border-color: var(--y7-border-strong);
    color: var(--y7-text-primary);
  }

  .empty {
    margin: 0;
    font-size: var(--y7-fs-md);
    color: var(--y7-text-muted);
    text-transform: lowercase;
  }

  .action-bar {
    display: flex;
    gap: var(--y7-sp-2);
    align-items: center;
    padding-top: var(--y7-sp-2);
  }

  .msg {
    margin: 0;
    font-size: var(--y7-fs-sm);
  }
  .msg.err {
    color: var(--y7-red);
  }
</style>
