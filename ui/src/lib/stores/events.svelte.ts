// Central event subscriber. Call `startEventDispatch()` once at app boot.
//
// The Rust side multiplexes every domain event on the single channel
// "y7ke://event" with a `kind` discriminator. This module owns the single
// `listen` registration and fans out into the per-domain stores.

import type { UnlistenFn } from "@tauri-apps/api/event";

import { onAppEvent } from "../bridge";
import type { AppEvent } from "../types";

import {
  applyContactAdded,
  applyContactRemoved,
} from "./contacts.svelte";
import { applyIdentityReady } from "./identity.svelte";
import {
  applyMessageReceived,
  applyMessageStatus,
} from "./chat.svelte";
import { applyPresence } from "./presence.svelte";
import {
  applyRequestReceived,
  applyRequestResolved,
} from "./requests.svelte";
import { openEmpty, router } from "./route.svelte";

interface EventState {
  started: boolean;
  starting: boolean;
  error: string | null;
  lastBackgroundError: string | null;
}

const state = $state<EventState>({
  started: false,
  starting: false,
  error: null,
  lastBackgroundError: null,
});

export const eventState = {
  get started(): boolean {
    return state.started;
  },
  get error(): string | null {
    return state.error;
  },
  get lastBackgroundError(): string | null {
    return state.lastBackgroundError;
  },
};

let unlisten: UnlistenFn | null = null;

export async function startEventDispatch(): Promise<void> {
  if (state.started || state.starting) return;
  state.starting = true;
  state.error = null;
  try {
    unlisten = await onAppEvent(dispatch);
    state.started = true;
  } catch (err) {
    state.error = err instanceof Error ? err.message : String(err);
  } finally {
    state.starting = false;
  }
}

export function stopEventDispatch(): void {
  if (unlisten) {
    unlisten();
    unlisten = null;
  }
  state.started = false;
}

function dispatch(ev: AppEvent): void {
  switch (ev.kind) {
    case "identity_ready":
      applyIdentityReady(ev.y7_id);
      break;
    case "request_received":
      applyRequestReceived(ev.y7_id, ev.greeting);
      break;
    case "request_resolved":
      applyRequestResolved(ev.y7_id, ev.resolution);
      break;
    case "contact_added":
      applyContactAdded(ev.y7_id);
      break;
    case "contact_removed":
      applyContactRemoved(ev.y7_id);
      // Eject from chat if it was with the removed peer.
      if (router.pane.kind === "chat" && router.pane.peerY7Id === ev.y7_id) {
        openEmpty();
      }
      break;
    case "message_received":
      applyMessageReceived({
        conversation_id: ev.conversation_id,
        message_id: ev.message_id,
        sender_y7_id: ev.sender_y7_id,
        timestamp_ms: ev.timestamp_ms,
        text: ev.text,
      });
      break;
    case "message_status_changed":
      applyMessageStatus(ev.message_id, ev.status as 0 | 1 | 2 | 3 | 4);
      break;
    case "presence_changed":
      applyPresence(ev.y7_id, ev.connection);
      break;
    case "background_error":
      state.lastBackgroundError = ev.message;
      break;
  }
}

export function clearBackgroundError(): void {
  state.lastBackgroundError = null;
}
