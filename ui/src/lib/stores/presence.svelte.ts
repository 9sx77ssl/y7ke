// Reactive y7_id → ConnectionKind map. SvelteMap so newly-inserted keys
// fire $derived in views — plain object proxy can miss adds. A parallel
// transport map carries the QUIC/TCP of the live connection so the chat
// header can render "Direct · QUIC"; the two maps move together (a single
// applyPresence call writes both) so they can't desync.

import { SvelteMap } from "svelte/reactivity";

import type { ConnectionKind } from "../types";
import type { Transport } from "../gen/Transport";

const presence = new SvelteMap<string, ConnectionKind>();
const transports = new SvelteMap<string, Transport | null>();

export function getPresence(y7Id: string): ConnectionKind {
  return presence.get(y7Id) ?? "offline";
}

export function getTransport(y7Id: string): Transport | null {
  return transports.get(y7Id) ?? null;
}

export function seedPresence(
  entries: Array<{
    y7_id: string;
    presence: ConnectionKind;
    transport?: Transport | null;
  }>,
): void {
  for (const e of entries) {
    presence.set(e.y7_id, e.presence);
    transports.set(e.y7_id, e.transport ?? null);
  }
}

export function applyPresence(
  y7Id: string,
  connection: ConnectionKind,
  transport: Transport | null = null,
): void {
  presence.set(y7Id, connection);
  transports.set(y7Id, transport);
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
