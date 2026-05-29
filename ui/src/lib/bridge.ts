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
import type { BootstrapEntry, Settings } from "./types-settings-stub";

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

// ── Settings ────────────────────────────────────────────────────────────────

export function getSettings(): Promise<Settings> {
  return call<Settings>("get_settings");
}

export function updateSettings(settings: Settings): Promise<void> {
  return call<void>("update_settings", { settings });
}

export function listBootstraps(): Promise<BootstrapEntry[]> {
  return call<BootstrapEntry[]>("list_bootstraps");
}

export function pingAllBootstraps(): Promise<BootstrapEntry[]> {
  return call<BootstrapEntry[]>("ping_all_bootstraps");
}

export function selectBestBootstrap(): Promise<string | null> {
  return call<string | null>("select_best_bootstrap");
}

// ── Diagnostics ─────────────────────────────────────────────────────────────

import type { DcutrStats } from "./gen/DcutrStats";
import type { NatReachability } from "./gen/NatReachability";
import type { ConnectionView } from "./gen/ConnectionView";
import type { DiagnosticsDetail } from "./gen/DiagnosticsDetail";

export function getDcutrStats(): Promise<DcutrStats> {
  return call<DcutrStats>("get_dcutr_stats");
}

export function getNatStatus(): Promise<NatReachability> {
  return call<NatReachability>("get_nat_status");
}

export function getDiagnosticsDetail(): Promise<DiagnosticsDetail> {
  return call<DiagnosticsDetail>("get_diagnostics_detail");
}

export function listActiveConnections(): Promise<ConnectionView[]> {
  return call<ConnectionView[]>("list_active_connections");
}

// ── Events ──────────────────────────────────────────────────────────────────

export function onAppEvent(
  handler: (ev: AppEvent) => void,
): Promise<UnlistenFn> {
  return tauriListen<AppEvent>(EVENT_CHANNEL, (event) => {
    handler(event.payload);
  });
}
