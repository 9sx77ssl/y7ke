// In-memory router for the main shell's center pane.

import { closeConversation } from "./chat.svelte";
import { log } from "../log";

const logger = log("route");

export type MainPane =
  | { kind: "empty" }
  | { kind: "chat"; peerY7Id: string }
  | { kind: "add_contact" }
  | { kind: "requests" }
  | { kind: "settings" }
  | { kind: "connectivity" };

const route = $state<{ pane: MainPane }>({ pane: { kind: "empty" } });

export const router = {
  get pane(): MainPane {
    return route.pane;
  },
};

export function openEmpty(): void {
  logger.debug("→ empty");
  closeConversation();
  route.pane = { kind: "empty" };
}

export function openChatWith(peerY7Id: string): void {
  logger.debug("→ chat", peerY7Id);
  // Skip the reset if we're already on this chat; otherwise closeConversation
  // would wipe state.messages and Chat.svelte's $effect wouldn't re-fire
  // because peerY7Id hasn't changed, leaving the user staring at an empty
  // pane until they navigate away and back.
  if (route.pane.kind === "chat" && route.pane.peerY7Id === peerY7Id) {
    return;
  }
  // Reset any stale chat-store state so the new chat starts fresh.
  closeConversation();
  route.pane = { kind: "chat", peerY7Id };
}

export function openAddContact(): void {
  logger.debug("→ add_contact");
  closeConversation();
  route.pane = { kind: "add_contact" };
}

export function openRequests(): void {
  logger.debug("→ requests");
  closeConversation();
  route.pane = { kind: "requests" };
}

export function openSettings(): void {
  logger.debug("→ settings");
  closeConversation();
  route.pane = { kind: "settings" };
}

export function openConnectivity(): void {
  logger.debug("→ connectivity");
  closeConversation();
  route.pane = { kind: "connectivity" };
}
