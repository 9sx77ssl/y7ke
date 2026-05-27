// Chat store — owns the message list for whichever contact is currently
// opened. There is only one active conversation in V1 (the right-hand pane);
// switching contacts wipes and reloads.
//
// Backend conversation IDs are derived from a pair of y7 IDs and are 16-byte
// hex strings. The frontend does NOT compute that — instead it tracks the
// active *peer y7_id* and re-fetches messages whenever `openConversation`
// runs. message_received events for any conversation id that's not the
// currently-open one are ignored here (they'd be reconciled the next time
// that contact's chat is opened, via list_messages from disk).
//
// To keep that "ignore other conversations" rule honest, we tag every loaded
// message list with its conversation_id and only accept events whose
// conversation_id matches the loaded set's first message — or, when the list
// is empty, we accept the first event matching our peer's tail and lock in.

import {
  listMessages as rpcListMessages,
  sendMessage as rpcSendMessage,
} from "../bridge";
import {
  MSG_FAILED,
  MSG_SENDING,
  type MessageStatus,
  type MessageView,
} from "../types";

const PAGE_LIMIT = 500;

interface ChatState {
  peerY7Id: string | null;
  conversationId: string | null;
  messages: MessageView[];
  loading: boolean;
  sending: boolean;
  error: string | null;
}

const state = $state<ChatState>({
  peerY7Id: null,
  conversationId: null,
  messages: [],
  loading: false,
  sending: false,
  error: null,
});

// Monotonic counter: only the latest openConversation may overwrite
// state.messages. Without this an in-flight load could clobber an optimistic
// placeholder that sendText added during the await window.
let loadGen = 0;

// Status updates can arrive before sendText has swapped the placeholderId for
// the realId — Rust's `push_one` runs in tokio::spawn and may ack before the
// JS-side invoke() promise resolves. Buffer any update whose message_id we
// don't currently hold; sendText's swap-success path drains the buffer.
const pendingStatus = new Map<string, MessageStatus>();

export const chat = {
  get peerY7Id(): string | null {
    return state.peerY7Id;
  },
  get conversationId(): string | null {
    return state.conversationId;
  },
  get messages(): MessageView[] {
    return state.messages;
  },
  get loading(): boolean {
    return state.loading;
  },
  get sending(): boolean {
    return state.sending;
  },
  get error(): string | null {
    return state.error;
  },
  get isOpen(): boolean {
    return state.peerY7Id !== null;
  },
};

/**
 * Open a conversation by peer y7_id. The backend command however expects a
 * conversation_id; in V1 we resolve it from the first listMessages call. If
 * the call returns nothing we still mark the chat as "open" so the composer
 * is usable for the first outbound message — the conversation_id is locked
 * in on the first inbound or outbound message thereafter.
 *
 * For V1 the wire signature for list_messages takes conversation_id directly.
 * Pending a backend addition of list_messages_with_peer, we fall back to
 * deriving the conversation_id from the peer y7_id by calling send_message in
 * dry-run mode... which doesn't exist. So for the very first call to a
 * brand-new contact, we pass the peer y7_id as the conversation_id and rely
 * on the backend to either resolve it or return an empty list. The error path
 * surfaces in `state.error` and the composer is still usable.
 */
export async function openConversation(peerY7Id: string): Promise<void> {
  const myGen = ++loadGen;
  state.peerY7Id = peerY7Id;
  state.conversationId = null;
  state.messages = [];
  state.error = null;
  state.loading = true;
  try {
    const items = await rpcListMessages(peerY7Id, PAGE_LIMIT);
    // Bail if a newer load has started OR the user switched peers.
    if (state.peerY7Id !== peerY7Id || myGen !== loadGen) return;

    // Merge with anything added to state.messages during the await window —
    // optimistic placeholders from sendText, or message_received events that
    // arrived before the load resolved. Otherwise a slow list_messages would
    // silently wipe the user's freshly-sent message from the UI.
    const itemIds = new Set(items.map((m) => m.message_id));
    const localOnly = state.messages.filter((m) => !itemIds.has(m.message_id));
    state.messages = [...items, ...localOnly].sort(
      (a, b) => a.timestamp_ms - b.timestamp_ms,
    );

    // Only adopt a conversation_id from a real server item; placeholders
    // carry "" and would otherwise poison the event filter.
    if (items.length > 0) {
      state.conversationId = items[0]!.conversation_id;
    }
  } catch (err) {
    if (state.peerY7Id === peerY7Id) {
      state.error = err instanceof Error ? err.message : String(err);
    }
  } finally {
    if (state.peerY7Id === peerY7Id) state.loading = false;
  }
}

export function closeConversation(): void {
  state.peerY7Id = null;
  state.conversationId = null;
  state.messages = [];
  state.error = null;
}

export async function sendText(text: string): Promise<void> {
  const peer = state.peerY7Id;
  if (peer === null) return;
  const trimmed = text.trim();
  if (trimmed.length === 0) return;

  state.sending = true;
  state.error = null;

  // Optimistic insert. The real message_id comes back from send_message; we
  // reconcile by replacing the placeholder once the command resolves.
  const placeholderId = `local-${crypto.randomUUID()}`;
  const placeholder: MessageView = {
    message_id: placeholderId,
    conversation_id: state.conversationId ?? "",
    sender_y7_id: "(me)",
    text: trimmed,
    timestamp_ms: Date.now(),
    status: MSG_SENDING,
    is_mine: true,
  };
  state.messages = [...state.messages, placeholder];

  try {
    const realId = await rpcSendMessage(peer, trimmed);
    // Replace placeholder; do not re-sort because we appended at the tail and
    // server-side timestamps are within ms of Date.now(). Only touch state if
    // we're still on the same peer — otherwise the map runs over the wrong
    // conversation (harmless no-op) and would also dirty state.sending.
    if (state.peerY7Id === peer) {
      // Swap placeholderId → realId; if a MessageStatusChanged for realId
      // arrived in the gap between insert+ack and the invoke() resolving,
      // apply it now so the bubble doesn't sit on Sending forever.
      const buffered = pendingStatus.get(realId);
      pendingStatus.delete(realId);
      state.messages = state.messages.map((m) => {
        if (m.message_id !== placeholderId) return m;
        return {
          ...m,
          message_id: realId,
          status: buffered ?? m.status,
        };
      });
    }
  } catch (err) {
    // Only surface the error on the conversation that triggered the send;
    // otherwise it bleeds into whichever chat the user switched to.
    if (state.peerY7Id === peer) {
      state.error = err instanceof Error ? err.message : String(err);
      state.messages = state.messages.map((m) =>
        m.message_id === placeholderId ? { ...m, status: MSG_FAILED } : m,
      );
    }
  } finally {
    // `sending` is the global send-in-flight flag; always release it. We must
    // not gate it on the peer matching, or a peer-switch mid-send would leave
    // Bob's composer disabled until Alice's RPC eventually resolves.
    state.sending = false;
  }
}

/** Event dispatch — message_received. */
export function applyMessageReceived(payload: {
  conversation_id: string;
  message_id: string;
  sender_y7_id: string;
  timestamp_ms: number;
  text: string;
}): void {
  // Ignore events for conversations we don't currently have open. The next
  // openConversation() call will re-fetch from disk and include the message.
  if (
    state.conversationId !== null &&
    payload.conversation_id !== state.conversationId
  ) {
    return;
  }

  // If we don't have a conversation_id locked in yet, accept the first event
  // that matches our peer (sender == peer for inbound).
  if (state.conversationId === null) {
    if (state.peerY7Id !== payload.sender_y7_id) return;
    state.conversationId = payload.conversation_id;
  }

  // Dedupe — backend can re-emit during sync reconciliation.
  if (state.messages.some((m) => m.message_id === payload.message_id)) return;

  const msg: MessageView = {
    message_id: payload.message_id,
    conversation_id: payload.conversation_id,
    sender_y7_id: payload.sender_y7_id,
    text: payload.text,
    timestamp_ms: payload.timestamp_ms,
    status: 1, // received messages arrive as Sent at minimum
    is_mine: false,
  };
  state.messages = [...state.messages, msg];
}

/** Event dispatch — message_status_changed. */
export function applyMessageStatus(messageId: string, status: MessageStatus): void {
  let matched = false;
  state.messages = state.messages.map((m) => {
    if (m.message_id !== messageId) return m;
    matched = true;
    return { ...m, status };
  });
  if (!matched) {
    // Likely a status event that beat sendText's placeholder→realId swap.
    // Stash it so the swap can pick it up. Capped at a small size to keep
    // the leak bounded if a peer somehow generates spurious status events.
    if (pendingStatus.size > 64) {
      const oldest = pendingStatus.keys().next().value;
      if (oldest !== undefined) pendingStatus.delete(oldest);
    }
    pendingStatus.set(messageId, status);
  }
}
