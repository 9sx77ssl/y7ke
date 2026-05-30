// Reactive y7_id → ConnectionKind map. SvelteMap so newly-inserted keys
// fire $derived in views — plain object proxy can miss adds. Parallel maps
// carry the QUIC/TCP transport, the IP family (v4/v6) and the provenance
// (how the connection was established) of the live connection so the chat
// header can render "DIRECT · QUIC · IPv6 · via DCUtR"; all maps move
// together (a single applyPresence call writes them all) so they can't
// desync.

import { SvelteMap } from "svelte/reactivity";

import type { ConnectionKind } from "../types";
import type { Transport } from "../gen/Transport";
import type { IpVersion } from "../gen/IpVersion";
import type { ConnectionOrigin } from "../gen/ConnectionOrigin";

const presence = new SvelteMap<string, ConnectionKind>();
const transports = new SvelteMap<string, Transport | null>();
const families = new SvelteMap<string, IpVersion | null>();
const origins = new SvelteMap<string, ConnectionOrigin | null>();

export function getPresence(y7Id: string): ConnectionKind {
  return presence.get(y7Id) ?? "offline";
}

export function getTransport(y7Id: string): Transport | null {
  return transports.get(y7Id) ?? null;
}

export function getIpFamily(y7Id: string): IpVersion | null {
  return families.get(y7Id) ?? null;
}

export function getOrigin(y7Id: string): ConnectionOrigin | null {
  return origins.get(y7Id) ?? null;
}

export function seedPresence(
  entries: Array<{
    y7_id: string;
    presence: ConnectionKind;
    transport?: Transport | null;
    ip_version?: IpVersion | null;
    origin?: ConnectionOrigin | null;
  }>,
): void {
  for (const e of entries) {
    presence.set(e.y7_id, e.presence);
    transports.set(e.y7_id, e.transport ?? null);
    families.set(e.y7_id, e.ip_version ?? null);
    origins.set(e.y7_id, e.origin ?? null);
  }
}

export function applyPresence(
  y7Id: string,
  connection: ConnectionKind,
  transport: Transport | null = null,
  ipVersion: IpVersion | null = null,
  origin: ConnectionOrigin | null = null,
): void {
  presence.set(y7Id, connection);
  transports.set(y7Id, transport);
  families.set(y7Id, ipVersion);
  origins.set(y7Id, origin);
}

export function presenceLabel(kind: ConnectionKind): string {
  switch (kind) {
    case "offline":
      return "Offline";
    case "connecting":
      return "Connecting";
    case "lan":
      return "LAN";
    case "internet":
      return "Internet";
    case "direct":
      return "Direct";
    case "relayed":
      return "Relay";
  }
}

// Short uppercase transport tag for the connection label ("QUIC" / "TCP").
export function transportLabel(t: Transport | null): string | null {
  if (t === null) return null;
  return t.toUpperCase();
}

// Short IP-family tag for the connection label ("IPv4" / "IPv6").
export function ipFamilyLabel(v: IpVersion | null): string | null {
  if (v === null) return null;
  return v === "v6" ? "IPv6" : "IPv4";
}

// Human phrase for how the connection was established. `null` for the
// origins that the kind+transport already make obvious (direct_dial,
// public_ipv4/6, unknown) — only DCUtR and relay add nuance worth a tag.
export function originTag(o: ConnectionOrigin | null): string | null {
  switch (o) {
    case "dcutr_upgrade":
      return "via DCUtR";
    case "relay_only":
      return "via relay";
    default:
      return null;
  }
}

// Full sentence for the connection tooltip ("how did we get here?").
export function originPhrase(o: ConnectionOrigin | null): string | null {
  switch (o) {
    case "direct_dial":
      return "established by a direct dial";
    case "dcutr_upgrade":
      return "hole-punched from a relay to a direct path (DCUtR)";
    case "relay_only":
      return "carried through a relay (no direct path yet)";
    case "public_ipv6":
      return "direct over a public IPv6 address";
    case "public_ipv4":
      return "direct over a public IPv4 address";
    case "unknown":
    case null:
      return null;
  }
}
