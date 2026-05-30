<script lang="ts" module>
  import type { ConnectionKind } from "../gen/ConnectionKind";
  import type { Transport } from "../gen/Transport";
  import type { IpVersion } from "../gen/IpVersion";
  import type { ConnectionOrigin } from "../gen/ConnectionOrigin";
  import {
    ipFamilyLabel,
    originTag,
    originPhrase,
  } from "../stores/presence.svelte";

  export interface ConnectionLabelProps {
    kind: ConnectionKind;
    /** Underlying transport of the live connection (QUIC / TCP). */
    transport?: Transport | null;
    /** IP family of the live connection (v4 / v6). */
    ipVersion?: IpVersion | null;
    /** HOW the connection was established (direct dial / DCUtR / relay / …). */
    origin?: ConnectionOrigin | null;
    /** Hostname shown in the relay tooltip. */
    relayHost?: string;
  }

  type Tone = "online" | "relayed";
</script>

<script lang="ts">
  // V2-A4/A5: small uppercase label that sits next to a peer's nickname
  // and surfaces *how* the connection is carried (LAN / Internet / Relay /
  // Direct), the transport (QUIC / TCP), the IP family (IPv4 / IPv6) and —
  // when it isn't obvious from the kind — the provenance (via DCUtR), e.g.
  // "DIRECT · QUIC · IPv6 · via DCUtR". The StatusDot already says
  // online-vs-offline; this label adds the path nuance. Hidden for offline /
  // connecting — the dot speaks for those.

  let {
    kind,
    transport = null,
    ipVersion = null,
    origin = null,
    relayHost = "bootstrap1.y7v.lol",
  }: ConnectionLabelProps = $props();

  const label = $derived(labelFor(kind, transport, ipVersion, origin));
  const tooltip = $derived(tooltipFor(kind, relayHost, transport, origin));
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

  function labelFor(
    k: ConnectionKind,
    t: Transport | null,
    v: IpVersion | null,
    o: ConnectionOrigin | null,
  ): string | null {
    const base = baseLabel(k);
    if (base === null) return null;
    const parts = [base];
    if (t !== null) parts.push(t.toUpperCase());
    const fam = ipFamilyLabel(v);
    if (fam !== null) parts.push(fam);
    const ot = originTag(o);
    // Only surface the origin tag when the kind doesn't already imply it:
    // RELAY already means relay_only; a DCUtR upgrade on a Direct line is
    // the interesting case worth a "via DCUtR" tag.
    if (ot !== null && o === "dcutr_upgrade") parts.push(ot);
    return parts.join(" · ");
  }

  function tooltipFor(
    k: ConnectionKind,
    host: string,
    t: Transport | null,
    o: ConnectionOrigin | null,
  ): string {
    const via = t === null ? "" : ` over ${t.toUpperCase()}`;
    const how = originPhrase(o);
    const howSuffix = how === null ? "" : ` — ${how}`;
    switch (k) {
      case "lan":
        return `connected over LAN (mDNS-discovered)${via}${howSuffix}`;
      case "internet":
        return `direct internet connection${via}${howSuffix}`;
      case "relayed":
        return `relayed via ${host}${via} — end-to-end encrypted, latency may be slightly higher`;
      case "direct":
        return `direct p2p connection${via}${howSuffix}`;
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
