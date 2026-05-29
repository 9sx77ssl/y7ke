// Identity store — surfaces the local `y7:` ID once the backend has generated
// it. Two state machines collapse into one boolean (`isReady`):
//   - cold start: backend reads existing key from SQLite, command returns.
//   - first launch: backend generates a key, emits identity_ready, then
//     subsequent get_my_id calls return the same string.
//
// Views poll `loadIdentity()` on mount; the events store also forwards
// identity_ready to `applyIdentityReady` so first-launch flows show the ID the
// moment the backend has it.

import { getMyId } from "../bridge";

interface IdentityState {
  y7Id: string | null;
  loading: boolean;
  error: string | null;
}

const state = $state<IdentityState>({
  y7Id: null,
  loading: false,
  error: null,
});

export const identity = {
  get y7Id(): string | null {
    return state.y7Id;
  },
  get loading(): boolean {
    return state.loading;
  },
  get error(): string | null {
    return state.error;
  },
  get isReady(): boolean {
    return state.y7Id !== null;
  },
};

// Non-reactive in-flight guard. Must NOT read the reactive `state.loading`:
// loadIdentity is sometimes called from a reactive context, and reading a
// tracked flag here (while the finally block writes it) is exactly what turned
// the App boot effect into a listener-flapping loop. The module-level boolean
// dedups concurrent calls without subscribing any caller to `loading`.
let inFlight = false;
export async function loadIdentity(): Promise<void> {
  if (inFlight) return;
  inFlight = true;
  state.loading = true;
  state.error = null;
  try {
    const id = await getMyId();
    state.y7Id = id;
  } catch (err) {
    state.error = err instanceof Error ? err.message : String(err);
  } finally {
    state.loading = false;
    inFlight = false;
  }
}

/** Called by the events dispatcher when `identity_ready` arrives. */
export function applyIdentityReady(y7Id: string): void {
  state.y7Id = y7Id;
  state.error = null;
}
