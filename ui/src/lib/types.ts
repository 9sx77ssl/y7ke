// Re-export auto-generated types (see crates/*/src/*.rs `#[derive(TS)]`).
// Files under ./gen/ are produced by ts-rs during `cargo test` and must not
// be edited by hand. This file adds the few wire-stable constants the
// generated bindings don't carry (status integers, channel name).

export type { AppEvent } from "./gen/AppEvent";
export type { ContactStatus } from "./gen/ContactStatus";
export type { ConnectionKind } from "./gen/ConnectionKind";
export type { ContactView } from "./gen/ContactView";
export type { RequestResolution } from "./gen/RequestResolution";
export type { RequestView } from "./gen/RequestView";
export type { MessageView } from "./gen/MessageView";

// Discriminator strings used by the AppEvent union — same shape as the
// Rust `serde(tag = "kind", rename_all = "snake_case")` emits.
export type IdentityReadyEvent = Extract<
  import("./gen/AppEvent").AppEvent,
  { kind: "identity_ready" }
>;
export type RequestReceivedEvent = Extract<
  import("./gen/AppEvent").AppEvent,
  { kind: "request_received" }
>;
export type RequestResolvedEvent = Extract<
  import("./gen/AppEvent").AppEvent,
  { kind: "request_resolved" }
>;
export type ContactAddedEvent = Extract<
  import("./gen/AppEvent").AppEvent,
  { kind: "contact_added" }
>;
export type ContactRemovedEvent = Extract<
  import("./gen/AppEvent").AppEvent,
  { kind: "contact_removed" }
>;
export type MessageReceivedEvent = Extract<
  import("./gen/AppEvent").AppEvent,
  { kind: "message_received" }
>;
export type MessageStatusChangedEvent = Extract<
  import("./gen/AppEvent").AppEvent,
  { kind: "message_status_changed" }
>;
export type PresenceChangedEvent = Extract<
  import("./gen/AppEvent").AppEvent,
  { kind: "presence_changed" }
>;
export type BackgroundErrorEvent = Extract<
  import("./gen/AppEvent").AppEvent,
  { kind: "background_error" }
>;

// Message status integers — must match the Rust `MessageStatus` repr(i64).
export const MSG_SENDING = 0;
export const MSG_SENT = 1;
export const MSG_DELIVERED = 2;
export const MSG_SYNCED = 3;
export const MSG_FAILED = 4;

export type MessageStatus = 0 | 1 | 2 | 3 | 4;

export const EVENT_CHANNEL = "y7ke://event";
