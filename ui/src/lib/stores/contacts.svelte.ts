// Contacts store — the source of truth for the sidebar list. Refreshes on
// demand from `list_contacts` and reacts to contact_added events.

import { deleteContact as rpcDelete, listContacts } from "../bridge";
import type { ContactView } from "../types";
import { seedPresence } from "./presence.svelte";

interface ContactsState {
  items: ContactView[];
  loading: boolean;
  error: string | null;
  loadedOnce: boolean;
}

const state = $state<ContactsState>({
  items: [],
  loading: false,
  error: null,
  loadedOnce: false,
});

export const contacts = {
  get items(): ContactView[] {
    return state.items;
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
  get accepted(): ContactView[] {
    return state.items.filter((c) => c.status === "accepted");
  },
  /**
   * Everything the sidebar should render: accepted contacts and any still-
   * pending ones (so the sender sees their pending peer immediately after
   * `send_contact_request` instead of waiting for them to message back).
   * Excludes `blocked` and `removed`.
   */
  get visible(): ContactView[] {
    return state.items.filter(
      (c) =>
        c.status === "accepted" ||
        c.status === "pending_out" ||
        c.status === "pending_in",
    );
  },
};

export async function refreshContacts(): Promise<void> {
  if (state.loading) return;
  state.loading = true;
  state.error = null;
  try {
    const items = await listContacts();
    state.items = items;
    state.loadedOnce = true;
    seedPresence(items);
  } catch (err) {
    state.error = err instanceof Error ? err.message : String(err);
  } finally {
    state.loading = false;
  }
}

/** Optimistic upsert called from event dispatch (`contact_added`). */
export function applyContactAdded(_y7Id: string): void {
  // The backend's contact_added event doesn't carry the full row, so the
  // cheapest correct thing is to re-fetch. refreshContacts is idempotent.
  void refreshContacts();
}

/** Event dispatch — contact_removed. Refresh + eject from chat if open. */
export function applyContactRemoved(_y7Id: string): void {
  void refreshContacts();
}

export function findContact(y7Id: string): ContactView | undefined {
  return state.items.find((c) => c.y7_id === y7Id);
}

export async function deleteContactAction(y7Id: string): Promise<void> {
  await rpcDelete(y7Id);
  await refreshContacts();
}
