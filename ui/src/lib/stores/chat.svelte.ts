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
  state.peerY7Id = peerY7Id;
  state.conversationId = null;
  state.messages = [];
  state.error = null;
  state.loading = true;
  try {
    // In V1 the conversation_id is a 16-byte hex; the backend exposes it on
    // each MessageView. We pass the peer y7_id and let the backend resolve.
    // If the backend hasn't wired this yet the call rejects -> shown inline.
    const items = await rpcListMessages(peerY7Id, PAGE_LIMIT);
    if (state.peerY7Id !== peerY7Id) return; // user switched while loading
    state.messages = items;
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
    // server-side timestamps are within ms of Date.now().
    state.messages = state.messages.map((m) =>
      m.message_id === placeholderId ? { ...m, message_id: realId } : m,
    );
  } catch (err) {
    state.error = err instanceof Error ? err.message : String(err);
    state.messages = state.messages.map((m) =>
      m.message_id === placeholderId ? { ...m, status: MSG_FAILED } : m,
    );
  } finally {
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
  state.messages = state.messages.map((m) =>
    m.message_id === messageId ? { ...m, status } : m,
  );
}
