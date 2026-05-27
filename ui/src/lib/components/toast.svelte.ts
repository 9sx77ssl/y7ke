/**
 * Toast queue — single source of truth used by `Toaster.svelte`.
 *
 * Usage:
 *   import { toast } from "$lib/components/toast.svelte";
 *   toast.success("copied");
 *   toast.error("something broke");
 */

export type ToastTone = "info" | "success" | "error";

export interface Toast {
  id: number;
  tone: ToastTone;
  message: string;
}

interface ToastState {
  queue: Toast[];
}

const state: ToastState = $state({ queue: [] });
let nextId = 0;
const DEFAULT_DURATION_MS = 2400;

function push(tone: ToastTone, message: string, durationMs: number = DEFAULT_DURATION_MS): void {
  const id = ++nextId;
  state.queue = [...state.queue, { id, tone, message }];
  window.setTimeout(() => {
    state.queue = state.queue.filter((t) => t.id !== id);
  }, durationMs);
}

export const toast = {
  get queue(): readonly Toast[] {
    return state.queue;
  },
  info(message: string, durationMs?: number): void {
    push("info", message, durationMs);
  },
  success(message: string, durationMs?: number): void {
    push("success", message, durationMs);
  },
  error(message: string, durationMs?: number): void {
    push("error", message, durationMs);
  },
};
