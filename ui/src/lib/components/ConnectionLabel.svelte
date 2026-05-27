<script lang="ts" module>
  import type { ConnectionKind } from "../gen/ConnectionKind";

  export interface ConnectionLabelProps {
    kind: ConnectionKind;
    /** Hostname shown in the relay tooltip. */
    relayHost?: string;
  }

  type Tone = "online" | "relayed";
</script>

<script lang="ts">
  // V2-A4: small uppercase label that sits next to a peer's nickname and
  // surfaces *how* the connection is carried (LAN / Internet / Relay /
  // Direct). The StatusDot already says online-vs-offline; this label
  // adds the transport-kind nuance. Hidden for offline / connecting —
  // the dot speaks for those.

  let { kind, relayHost = "bootstrap1.y7v.lol" }: ConnectionLabelProps =
    $props();

  const label = $derived(labelFor(kind));
  const tooltip = $derived(tooltipFor(kind, relayHost));
  const tone = $derived(toneFor(kind));

  function labelFor(k: ConnectionKind): string | null {
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

  function tooltipFor(k: ConnectionKind, host: string): string {
    switch (k) {
      case "lan":
        return "connected over LAN (mDNS-discovered)";
      case "internet":
        return "direct internet connection";
      case "relayed":
        return `relayed via ${host} — end-to-end encrypted, latency may be slightly higher`;
      case "direct":
        return "direct connection (NAT-traversed)";
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
