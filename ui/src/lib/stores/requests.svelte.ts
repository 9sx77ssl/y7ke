// Requests store — surfaces inbound + outbound pending contact requests. The
// backend exposes both via the single `list_pending_requests` command;
// direction is encoded in the row.

import {
  acceptRequest as rpcAccept,
  cancelRequest as rpcCancel,
  listPendingRequests,
  rejectRequest as rpcReject,
  sendContactRequest as rpcSend,
} from "../bridge";
import type { RequestView } from "../types";
import { refreshContacts } from "./contacts.svelte";
import { toast } from "../components/toast.svelte";

interface RequestsState {
  items: RequestView[];
  loading: boolean;
  error: string | null;
  loadedOnce: boolean;
}

const state = $state<RequestsState>({
  items: [],
  loading: false,
  error: null,
  loadedOnce: false,
});

export const requests = {
  get items(): RequestView[] {
    return state.items;
  },
  get incoming(): RequestView[] {
    return state.items.filter((r) => r.direction === "incoming");
  },
  get outgoing(): RequestView[] {
    return state.items.filter((r) => r.direction === "outgoing");
  },
  get incomingCount(): number {
    return state.items.reduce((n, r) => (r.direction === "incoming" ? n + 1 : n), 0);
  },
  get loading(): boolean {
    return state.loading;
  },
  get error(): string | null {
    return state.error;
  },
  get loadedOnce(): boolean {
    return state.loadedOnce;
  },
};

export async function refreshRequests(): Promise<void> {
  if (state.loading) return;
  state.loading = true;
  state.error = null;
  try {
    state.items = await listPendingRequests();
    state.loadedOnce = true;
  } catch (err) {
    state.error = err instanceof Error ? err.message : String(err);
  } finally {
    state.loading = false;
  }
}

export async function sendContactRequestAction(
  y7Id: string,
  greeting: string | null,
): Promise<void> {
  // Errors propagate to the caller view so it can show inline feedback.
  await rpcSend(y7Id, greeting);
  // The contact row (pending_out) is inserted by the backend; the sidebar
  // needs to know about it immediately so the user has something to click
  // even before the peer accepts.
  await Promise.all([refreshRequests(), refreshContacts()]);
}

export async function acceptRequestAction(requestId: number): Promise<void> {
  await rpcAccept(requestId);
  await Promise.all([refreshRequests(), refreshContacts()]);
}

export async function rejectRequestAction(requestId: number): Promise<void> {
  await rpcReject(requestId);
  // Reject marks the peer Blocked; refresh contacts too so the stale
  // pending row drops from the sidebar immediately (not on the next
  // unrelated presence event).
  await Promise.all([refreshRequests(), refreshContacts()]);
}

/**
 * Cancel a pending OUTGOING contact request. Errors propagate; the calling
 * view is responsible for catching + toasting (the backend command may not
 * be wired yet during dev).
 */
export async function cancelRequestAction(requestId: number): Promise<void> {
  await rpcCancel(requestId);
  // Cancel marks the contact Removed — refresh both so the row clears now.
  await Promise.all([refreshRequests(), refreshContacts()]);
}

/** Event dispatch hooks — see events.svelte.ts. */
export function applyRequestReceived(_y7Id: string, _greeting: string | null): void {
  void refreshRequests();
}

// A request was resolved — locally OR by the peer. The backend emits this on
// every resolution path, so it is the single source of truth for resolution
// feedback: surface the outcome (the three RequestResolution variants) and
// refresh BOTH stores. Contacts must refresh because reject→Blocked and
// cancel→Removed mutate the contact row; without it a stale "pending" row
// lingers in the sidebar until an unrelated presence event fires.
export function applyRequestResolved(_y7Id: string, resolution: string): void {
  void Promise.all([refreshRequests(), refreshContacts()]);
  switch (resolution) {
    case "accepted":
      toast.success("contact request accepted");
      break;
    case "rejected":
      toast.info("contact request rejected");
      break;
    case "cancelled":
      toast.info("contact request cancelled");
      break;
  }
}
