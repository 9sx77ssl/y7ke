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
import { log } from "../log";
import {
  MSG_FAILED,
  MSG_SENDING,
  type MessageStatus,
  type MessageView,
} from "../types";

const logger = log("chat-store");
const PAGE_LIMIT = 500;

interface ChatState {
  peerY7Id: string | null;
  conversationId: string | null;
  messages: MessageView[];
  loading: boolean;
  sending: boolean;
  error: string | null;
}

// Direct $state export so Svelte 5 reactivity stays simple: components
// access `chat.messages` and the proxy registers the read inside the render
// effect. Previously we wrapped state in a plain object with getters, which
// broke reactivity in the production bundle — Chat.svelte's {#each} stopped
// re-rendering after `chat.messages = ...` assignments even though the
// store logs showed the array was updated.
export const chat = $state<ChatState>({
  peerY7Id: null,
  conversationId: null,
  messages: [],
  loading: false,
  sending: false,
  error: null,
});

// Monotonic counter: only the latest openConversation may overwrite
// chat.messages. Without this an in-flight load could clobber an optimistic
// placeholder that sendText added during the await window.
let loadGen = 0;

// Status updates can arrive before sendText has swapped the placeholderId for
// the realId — Rust's `push_one` runs in tokio::spawn and may ack before the
// JS-side invoke() promise resolves. Buffer any update whose message_id we
// don't currently hold; sendText's swap-success path drains the buffer.
const pendingStatus = new Map<string, MessageStatus>();

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
 * surfaces in `chat.error` and the composer is still usable.
 */
export async function openConversation(peerY7Id: string): Promise<void> {
  const myGen = ++loadGen;
  logger.debug("openConversation: enter", { peer: peerY7Id, gen: myGen });
  chat.peerY7Id = peerY7Id;
  chat.conversationId = null;
  chat.messages = [];
  chat.error = null;
  chat.loading = true;
  try {
    const items = await rpcListMessages(peerY7Id, PAGE_LIMIT);
    logger.debug("openConversation: list_messages resolved", {
      peer: peerY7Id,
      gen: myGen,
      currentGen: loadGen,
      currentPeer: chat.peerY7Id,
      count: items.length,
    });
    // Bail if a newer load has started OR the user switched peers.
    if (chat.peerY7Id !== peerY7Id || myGen !== loadGen) {
      logger.warn("openConversation: bailing (newer load or peer switched)", {
        peer: peerY7Id,
        gen: myGen,
        currentGen: loadGen,
        currentPeer: chat.peerY7Id,
      });
      return;
    }

    // Merge with anything added to chat.messages during the await window —
    // optimistic placeholders from sendText, or message_received events that
    // arrived before the load resolved. Otherwise a slow list_messages would
    // silently wipe the user's freshly-sent message from the UI.
    const itemIds = new Set(items.map((m) => m.message_id));
    const localOnly = chat.messages.filter((m) => !itemIds.has(m.message_id));
    const merged = [...items, ...localOnly].sort(
      (a, b) => a.timestamp_ms - b.timestamp_ms,
    );
    chat.messages = merged;
    logger.debug("openConversation: merged + applied", {
      peer: peerY7Id,
      items: items.length,
      localOnly: localOnly.length,
      total: merged.length,
      stateLen: chat.messages.length,
    });

    // Only adopt a conversation_id from a real server item; placeholders
    // carry "" and would otherwise poison the event filter.
    if (items.length > 0) {
      chat.conversationId = items[0]!.conversation_id;
    }
  } catch (err) {
    logger.error("openConversation: list_messages failed", {
      peer: peerY7Id,
      err: err instanceof Error ? err.message : String(err),
    });
    if (chat.peerY7Id === peerY7Id) {
      chat.error = err instanceof Error ? err.message : String(err);
    }
  } finally {
    if (chat.peerY7Id === peerY7Id) chat.loading = false;
  }
}

export function closeConversation(): void {
  logger.debug("closeConversation", {
    peer: chat.peerY7Id,
    prevMsgCount: chat.messages.length,
  });
  chat.peerY7Id = null;
  chat.conversationId = null;
  chat.messages = [];
  chat.error = null;
}

export async function sendText(text: string): Promise<void> {
  const peer = chat.peerY7Id;
  logger.debug("sendText: enter", {
    peer,
    sending: chat.sending,
    len: text.length,
  });
  if (peer === null) {
    logger.warn("sendText: aborted — no peer in chat state");
    return;
  }
  const trimmed = text.trim();
  if (trimmed.length === 0) {
    logger.debug("sendText: aborted — empty text after trim");
    return;
  }

  chat.sending = true;
  chat.error = null;

  // Optimistic insert. The real message_id comes back from send_message; we
  // reconcile by replacing the placeholder once the command resolves.
  const placeholderId = `local-${crypto.randomUUID()}`;
  const placeholder: MessageView = {
    message_id: placeholderId,
    conversation_id: chat.conversationId ?? "",
    sender_y7_id: "(me)",
    text: trimmed,
    timestamp_ms: Date.now(),
    status: MSG_SENDING,
    is_mine: true,
  };
  chat.messages = [...chat.messages, placeholder];
  logger.debug("sendText: placeholder inserted", {
    placeholderId,
    msgCount: chat.messages.length,
  });

  try {
    const realId = await rpcSendMessage(peer, trimmed);
    logger.debug("sendText: rpcSendMessage resolved", {
      realId,
      currentPeer: chat.peerY7Id,
      expectedPeer: peer,
    });
    // Replace placeholder; do not re-sort because we appended at the tail and
    // server-side timestamps are within ms of Date.now(). Only touch state if
    // we're still on the same peer — otherwise the map runs over the wrong
    // conversation (harmless no-op) and would also dirty chat.sending.
    if (chat.peerY7Id === peer) {
      // Swap placeholderId → realId; if a MessageStatusChanged for realId
      // arrived in the gap between insert+ack and the invoke() resolving,
      // apply it now so the bubble doesn't sit on Sending forever.
      const buffered = pendingStatus.get(realId);
      pendingStatus.delete(realId);
      let matched = false;
      chat.messages = chat.messages.map((m) => {
        if (m.message_id !== placeholderId) return m;
        matched = true;
        return {
          ...m,
          message_id: realId,
          status: buffered ?? m.status,
        };
      });
      logger.debug("sendText: swap placeholder → realId", {
        realId,
        matched,
        bufferedStatus: buffered,
        msgCount: chat.messages.length,
      });
    } else {
      logger.warn("sendText: peer changed during await — no swap", {
        was: peer,
        now: chat.peerY7Id,
      });
    }
  } catch (err) {
    logger.error("sendText: rpcSendMessage failed", {
      err: err instanceof Error ? err.message : String(err),
    });
    // Only surface the error on the conversation that triggered the send;
    // otherwise it bleeds into whichever chat the user switched to.
    if (chat.peerY7Id === peer) {
      chat.error = err instanceof Error ? err.message : String(err);
      chat.messages = chat.messages.map((m) =>
        m.message_id === placeholderId ? { ...m, status: MSG_FAILED } : m,
      );
    }
  } finally {
    // `sending` is the global send-in-flight flag; always release it. We must
    // not gate it on the peer matching, or a peer-switch mid-send would leave
    // Bob's composer disabled until Alice's RPC eventually resolves.
    chat.sending = false;
    logger.debug("sendText: exit", {
      sending: chat.sending,
      msgCount: chat.messages.length,
    });
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
  logger.debug("applyMessageReceived: called", {
    mid: payload.message_id,
    sender: payload.sender_y7_id,
    convId: payload.conversation_id,
    currentPeer: chat.peerY7Id,
    currentConvId: chat.conversationId,
  });
  // Ignore events for conversations we don't currently have open. The next
  // openConversation() call will re-fetch from disk and include the message.
  if (
    chat.conversationId !== null &&
    payload.conversation_id !== chat.conversationId
  ) {
    logger.debug("applyMessageReceived: filtered (conv mismatch)", {
      mid: payload.message_id,
    });
    return;
  }

  // If we don't have a conversation_id locked in yet, accept the first event
  // that matches our peer (sender == peer for inbound).
  if (chat.conversationId === null) {
    if (chat.peerY7Id !== payload.sender_y7_id) {
      logger.debug("applyMessageReceived: filtered (peer mismatch)", {
        mid: payload.message_id,
        peer: chat.peerY7Id,
        sender: payload.sender_y7_id,
      });
      return;
    }
    chat.conversationId = payload.conversation_id;
  }

  // Dedupe — backend can re-emit during sync reconciliation.
  if (chat.messages.some((m) => m.message_id === payload.message_id)) {
    logger.debug("applyMessageReceived: filtered (dupe)", {
      mid: payload.message_id,
    });
    return;
  }

  const msg: MessageView = {
    message_id: payload.message_id,
    conversation_id: payload.conversation_id,
    sender_y7_id: payload.sender_y7_id,
    text: payload.text,
    timestamp_ms: payload.timestamp_ms,
    status: 1, // received messages arrive as Sent at minimum
    is_mine: false,
  };
  chat.messages = [...chat.messages, msg];
  logger.debug("applyMessageReceived: applied", {
    mid: payload.message_id,
    msgCount: chat.messages.length,
  });
}

/** Event dispatch — message_status_changed. */
export function applyMessageStatus(messageId: string, status: MessageStatus): void {
  let matched = false;
  chat.messages = chat.messages.map((m) => {
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
    logger.debug("applyMessageStatus: buffered (no match)", {
      mid: messageId,
      status,
      pendingSize: pendingStatus.size,
    });
  } else {
    logger.debug("applyMessageStatus: applied", { mid: messageId, status });
  }
}
