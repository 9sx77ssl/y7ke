<script lang="ts" module>
  import type { ConnectionKind } from "../gen/ConnectionKind";
  import type { Transport } from "../gen/Transport";

  export interface ConnectionLabelProps {
    kind: ConnectionKind;
    /** Underlying transport of the live connection (QUIC / TCP). */
    transport?: Transport | null;
    /** Hostname shown in the relay tooltip. */
    relayHost?: string;
  }

  type Tone = "online" | "relayed";
</script>

<script lang="ts">
  // V2-A4/A5: small uppercase label that sits next to a peer's nickname
  // and surfaces *how* the connection is carried (LAN / Internet / Relay /
  // Direct) plus the transport (QUIC / TCP), e.g. "DIRECT · QUIC". The
  // StatusDot already says online-vs-offline; this label adds the
  // path+transport nuance. Hidden for offline / connecting — the dot
  // speaks for those.

  let {
    kind,
    transport = null,
    relayHost = "bootstrap1.y7v.lol",
  }: ConnectionLabelProps = $props();

  const label = $derived(labelFor(kind, transport));
  const tooltip = $derived(tooltipFor(kind, relayHost, transport));
  const tone = $derived(toneFor(kind));

  function baseLabel(k: ConnectionKind): string | null {
    switch (k) {
      case "lan":
        return "LAN";
      case "internet":
        return "INTERNET";
      case "relayed":
        return "RELAY";
      case "direct":
        return "DIRECT";
      case "offline":
      case "connecting":
        return null;
    }
  }

  function labelFor(k: ConnectionKind, t: Transport | null): string | null {
    const base = baseLabel(k);
    if (base === null) return null;
    return t === null ? base : `${base} · ${t.toUpperCase()}`;
  }

  function tooltipFor(
    k: ConnectionKind,
    host: string,
    t: Transport | null,
  ): string {
    const via = t === null ? "" : ` over ${t.toUpperCase()}`;
    switch (k) {
      case "lan":
        return `connected over LAN (mDNS-discovered)${via}`;
      case "internet":
        return `direct internet connection${via}`;
      case "relayed":
        return `relayed via ${host}${via} — end-to-end encrypted, latency may be slightly higher`;
      case "direct":
        return `direct p2p connection (hole-punched through bootstrap)${via}`;
      case "offline":
      case "connecting":
        return "";
    }
  }

  function toneFor(k: ConnectionKind): Tone | null {
    switch (k) {
      case "lan":
      case "internet":
      case "direct":
        return "online";
      case "relayed":
        return "relayed";
      case "offline":
      case "connecting":
        return null;
    }
  }
</script>

{#if label && tone}
  <span class="label tone-{tone}" title={tooltip}>{label}</span>
{/if}

<style>
  .label {
    font-size: var(--y7-fs-xs);
    font-weight: var(--y7-fw-semibold);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .tone-online {
    color: var(--y7-text-online);
  }
  .tone-relayed {
    color: var(--y7-text-relayed);
  }
</style>
