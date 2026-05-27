// Reactive y7_id → ConnectionKind map. SvelteMap so newly-inserted keys
// fire $derived in views — plain object proxy can miss adds.

import { SvelteMap } from "svelte/reactivity";

import type { ConnectionKind } from "../types";

const presence = new SvelteMap<string, ConnectionKind>();

export function getPresence(y7Id: string): ConnectionKind {
  return presence.get(y7Id) ?? "offline";
}

export function seedPresence(
  entries: Array<{ y7_id: string; presence: ConnectionKind }>,
): void {
  for (const e of entries) presence.set(e.y7_id, e.presence);
}

export function applyPresence(y7Id: string, connection: ConnectionKind): void {
  presence.set(y7Id, connection);
}

export function presenceLabel(kind: ConnectionKind): string {
  switch (kind) {
    case "offline":
      return "Offline";
    case "connecting":
      return "Connecting";
    case "lan":
      return "LAN";
    case "direct":
      return "Direct";
    case "relayed":
      return "Relay";
  }
}
