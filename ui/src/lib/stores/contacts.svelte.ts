// Contacts store — the source of truth for the sidebar list. Refreshes on
// demand from `list_contacts` and reacts to contact_added events.

import { listContacts } from "../bridge";
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

export function findContact(y7Id: string): ContactView | undefined {
  return state.items.find((c) => c.y7_id === y7Id);
}
