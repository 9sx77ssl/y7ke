// Typed wrappers around @tauri-apps/api `invoke` and `listen` so views and
// stores never touch raw command names / event names.
//
// Every wrapper rejects with a normalized `Error` whose message is the
// stringified Rust error. Callers are expected to .catch() and surface the
// failure inline; the bridge itself never throws synchronously.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";

import {
  EVENT_CHANNEL,
  type AppEvent,
  type ContactView,
  type MessageView,
  type RequestView,
} from "./types";

function normalizeError(err: unknown): Error {
  if (err instanceof Error) return err;
  if (typeof err === "string") return new Error(err);
  try {
    return new Error(JSON.stringify(err));
  } catch {
    return new Error(String(err));
  }
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await tauriInvoke<T>(cmd, args);
  } catch (err) {
    throw normalizeError(err);
  }
}

// ── Identity ────────────────────────────────────────────────────────────────

export function getMyId(): Promise<string> {
  return call<string>("get_my_id");
}

// ── Contacts ────────────────────────────────────────────────────────────────

export function listContacts(): Promise<ContactView[]> {
  return call<ContactView[]>("list_contacts");
}

// ── Requests ────────────────────────────────────────────────────────────────

export function listPendingRequests(): Promise<RequestView[]> {
  return call<RequestView[]>("list_pending_requests");
}

export function sendContactRequest(
  y7Id: string,
  greeting: string | null,
): Promise<void> {
  return call<void>("send_contact_request", { y7Id, greeting });
}

export function acceptRequest(requestId: number): Promise<void> {
  return call<void>("accept_request", { requestId });
}

export function rejectRequest(requestId: number): Promise<void> {
  return call<void>("reject_request", { requestId });
}

/**
 * Cancel a pending outgoing contact request.
 *
 * The backend `cancel_request` command may not exist yet during early
 * development; callers should `.catch()` and surface the error inline rather
 * than letting it crash the view.
 */
export function cancelRequest(requestId: number): Promise<void> {
  return call<void>("cancel_request", { requestId });
}

export function deleteContact(y7Id: string): Promise<void> {
  return call<void>("delete_contact", { y7Id });
}

// ── Logging ─────────────────────────────────────────────────────────────────

export type LogLevel = "trace" | "debug" | "info" | "warn" | "error";

export function logToBackend(
  level: LogLevel,
  target: string,
  message: string,
): void {
  // Fire-and-forget; we don't want UI logs to throw if the bridge is offline.
  void call("log_from_ui", { level, target, message }).catch(() => {});
}

// ── Messages ────────────────────────────────────────────────────────────────

export function listMessages(
  conversationId: string,
  limit: number,
): Promise<MessageView[]> {
  return call<MessageView[]>("list_messages", {
    conversationId,
    limit,
  });
}

export function sendMessage(toY7Id: string, text: string): Promise<string> {
  return call<string>("send_message", { toY7Id, text });
}

// ── Events ──────────────────────────────────────────────────────────────────

export function onAppEvent(
  handler: (ev: AppEvent) => void,
): Promise<UnlistenFn> {
  return tauriListen<AppEvent>(EVENT_CHANNEL, (event) => {
    handler(event.payload);
  });
}
