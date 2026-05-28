// Settings store — dial mode + bootstrap nodes. Mirrors the same shape as
// contacts.svelte / requests.svelte (state via $state, refresh* async,
// loadedOnce flag, error string).

import {
  getSettings,
  listBootstraps,
  pingAllBootstraps,
  selectBestBootstrap,
  updateSettings,
} from "../bridge";
import type {
  BootstrapEntry,
  DialMode,
  Settings,
} from "../types-settings-stub";
import { log } from "../log";

const logger = log("settings.store");

interface SettingsState {
  settings: Settings | null;
  bootstraps: BootstrapEntry[];
  loading: boolean;
  pinging: boolean;
  saving: boolean;
  error: string | null;
  loadedOnce: boolean;
}

const state = $state<SettingsState>({
  settings: null,
  bootstraps: [],
  loading: false,
  pinging: false,
  saving: false,
  error: null,
  loadedOnce: false,
});

export const settingsStore = {
  get settings(): Settings | null {
    return state.settings;
  },
  get bootstraps(): BootstrapEntry[] {
    return state.bootstraps;
  },
  get loading(): boolean {
    return state.loading;
  },
  get pinging(): boolean {
    return state.pinging;
  },
  get saving(): boolean {
    return state.saving;
  },
  get error(): string | null {
    return state.error;
  },
  get loadedOnce(): boolean {
    return state.loadedOnce;
  },
};

export async function refreshSettings(): Promise<void> {
  if (state.loading) return;
  state.loading = true;
  state.error = null;
  try {
    const [s, b] = await Promise.all([getSettings(), listBootstraps()]);
    state.settings = s;
    state.bootstraps = b;
    state.loadedOnce = true;
    logger.debug("refreshed", `dial_mode=${s.dial_mode}`, `bootstraps=${b.length}`);
  } catch (err) {
    state.error = err instanceof Error ? err.message : String(err);
    logger.error("refresh failed", state.error);
  } finally {
    state.loading = false;
  }
}

export async function saveSettings(
  dialMode: DialMode,
  extraBootstraps: string[],
): Promise<void> {
  state.saving = true;
  state.error = null;
  try {
    const next: Settings = {
      dial_mode: dialMode,
      extra_bootstraps: extraBootstraps,
    };
    await updateSettings(next);
    state.settings = next;
    logger.info("saved");
  } finally {
    state.saving = false;
  }
}

export async function pingAll(): Promise<BootstrapEntry[]> {
  state.pinging = true;
  state.error = null;
  try {
    const updated = await pingAllBootstraps();
    state.bootstraps = updated;
    logger.info("ping complete", `entries=${updated.length}`);
    return updated;
  } finally {
    state.pinging = false;
  }
}

export async function pickBestBootstrap(): Promise<string | null> {
  return selectBestBootstrap();
}
