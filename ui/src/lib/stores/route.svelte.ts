// Lightweight in-memory router for the main shell's center pane.
// V1 has three view kinds — see `MainPane` below. Hash routing or any
// browser-history-bound system would be overkill for a Tauri desktop app.

export type MainPane =
  | { kind: "empty" }
  | { kind: "chat"; peerY7Id: string }
  | { kind: "add_contact" }
  | { kind: "requests" };

const route = $state<{ pane: MainPane }>({ pane: { kind: "empty" } });

export const router = {
  get pane(): MainPane {
    return route.pane;
  },
};

export function openEmpty(): void {
  route.pane = { kind: "empty" };
}

export function openChatWith(peerY7Id: string): void {
  route.pane = { kind: "chat", peerY7Id };
}

export function openAddContact(): void {
  route.pane = { kind: "add_contact" };
}

export function openRequests(): void {
  route.pane = { kind: "requests" };
}
