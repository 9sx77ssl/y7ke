import { defineConfig, type Plugin } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Force a FULL page reload on every dev change instead of partial HMR.
//
// The app's singleton Svelte stores (chat / events / contacts / requests /
// presence / settings) hold module-level `$state` plus the single Tauri
// event listener. Partial HMR re-evaluates those modules and splits state
// across generations: events then mutate a dead store instance while the
// live components read a stale one — so inbound messages and status updates
// land in the store (msgCount increments in the logs) but the chat {#each}
// never re-renders until you re-enter, and you see a storm of "[TAURI]
// Couldn't find callback id … app reloaded while Rust running async". A full
// reload rebuilds ONE clean module graph each save; the Rust backend (and the
// libp2p swarm) stays up across the webview reload. Dev-only — `build` never
// calls handleHotUpdate, so the production bundle is unaffected.
const fullReloadOnChange: Plugin = {
  name: "y7ke-full-reload-on-change",
  enforce: "post",
  handleHotUpdate({ server }) {
    server.ws.send({ type: "full-reload" });
    return [];
  },
};

// Tauri expects a fixed port; fail if the port is in use rather than silently changing.
export default defineConfig({
  plugins: [svelte(), fullReloadOnChange],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: process.env["TAURI_ENV_PLATFORM"] === "windows" ? "chrome105" : "safari13",
    minify: !process.env["TAURI_ENV_DEBUG"] ? "esbuild" : false,
    sourcemap: !!process.env["TAURI_ENV_DEBUG"],
  },
});
