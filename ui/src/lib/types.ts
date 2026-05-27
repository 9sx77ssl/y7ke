// Shared TypeScript types mirroring the Rust Tauri command + event surface.
// Keep these in lock-step with `src-tauri/src/commands.rs` and the Rust
// `AppEvent` enum which serializes as `{ kind, ...payload }` (snake_case tag).

export type ContactStatus =
  | "accepted"
  | "pending_out"
  | "pending_in"
  | "blocked"
  | "removed";

export type ConnectionKind = "offline" | "connecting" | "lan";

export interface ContactView {
  y7_id: string;
  nickname: string | null;
  status: ContactStatus;
  added_at: number;
  presence: ConnectionKind;
}

export type RequestDirection = "incoming" | "outgoing";

export interface RequestView {
  id: number;
  direction: RequestDirection;
  peer_y7_id: string;
  initial_text: string | null;
  created_at: number;
}

// Message status integers — must match the Rust enum.
export const MSG_SENDING = 0;
export const MSG_SENT = 1;
export const MSG_DELIVERED = 2;
export const MSG_SYNCED = 3;
export const MSG_FAILED = 4;

export type MessageStatus = 0 | 1 | 2 | 3 | 4;

export interface MessageView {
  message_id: string;
  conversation_id: string;
  sender_y7_id: string;
  text: string;
  timestamp_ms: number;
  status: MessageStatus;
  is_mine: boolean;
}

// Tauri event payloads. All arrive on the single channel "y7ke://event"; the
// `kind` discriminator selects the variant.

export interface IdentityReadyEvent {
  kind: "identity_ready";
  y7_id: string;
}

export interface RequestReceivedEvent {
  kind: "request_received";
  y7_id: string;
  greeting: string | null;
}

export type RequestResolution = "accepted" | "rejected" | "cancelled";

export interface RequestResolvedEvent {
  kind: "request_resolved";
  y7_id: string;
  resolution: RequestResolution;
}

export interface ContactAddedEvent {
  kind: "contact_added";
  y7_id: string;
}

export interface ContactRemovedEvent {
  kind: "contact_removed";
  y7_id: string;
}

export interface MessageReceivedEvent {
  kind: "message_received";
  conversation_id: string;
  message_id: string;
  sender_y7_id: string;
  timestamp_ms: number;
  text: string;
}

export interface MessageStatusChangedEvent {
  kind: "message_status_changed";
  message_id: string;
  status: MessageStatus;
}

export interface PresenceChangedEvent {
  kind: "presence_changed";
  y7_id: string;
  connection: ConnectionKind;
}

export interface BackgroundErrorEvent {
  kind: "background_error";
  message: string;
}

export type AppEvent =
  | IdentityReadyEvent
  | RequestReceivedEvent
  | RequestResolvedEvent
  | ContactAddedEvent
  | ContactRemovedEvent
  | MessageReceivedEvent
  | MessageStatusChangedEvent
  | PresenceChangedEvent
  | BackgroundErrorEvent;

export const EVENT_CHANNEL = "y7ke://event";
