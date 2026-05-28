// Settings wire types. The backend agent's ts-rs generation has landed real
// definitions under `./gen/`, so we re-export those here. This indirection
// stays so views can import from a single stable spot and so the place where
// `bigint` (ts-rs maps Rust `u64`) crosses into UI code is obvious.

export type { DialMode } from "./gen/DialMode";
export type { Settings } from "./gen/Settings";
export type { BootstrapEntry } from "./gen/BootstrapEntry";
