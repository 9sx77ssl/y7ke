// Frontend log helper. Mirrors to console + bridges to backend tracing so
// everything ends up in the Tauri stdout / log file, and keeps a small
// in-memory ring buffer the "copy diagnostics" button reads.
//
// `target` is the structured category (e.g. "Sidebar", "Connectivity",
// "net"). Repeated identical debug/trace lines are rate-limited so a
// reconnect storm can't flood the console or the ring buffer.

import { logToBackend, type LogLevel } from "./bridge";

interface LogEntry {
  t: number;
  level: LogLevel;
  target: string;
  message: string;
}

const RING_CAP = 400;
const ring: LogEntry[] = [];

// Rate-limit: collapse identical consecutive lines emitted within this
// window. Applies to debug/trace only — info/warn/error always emit.
const DEDUP_MS = 1500;
let lastKey = "";
let lastAt = 0;

function pushRing(level: LogLevel, target: string, message: string): void {
  ring.push({ t: Date.now(), level, target, message });
  if (ring.length > RING_CAP) ring.shift();
}

function emit(level: LogLevel, target: string, ...parts: unknown[]): void {
  const message = parts
    .map((p) => (typeof p === "string" ? p : JSON.stringify(p)))
    .join(" ");

  // Rate-limit noisy repeats (debug/trace) so storms don't flood.
  if (level === "debug" || level === "trace") {
    const key = `${level}|${target}|${message}`;
    const now = Date.now();
    if (key === lastKey && now - lastAt < DEDUP_MS) return;
    lastKey = key;
    lastAt = now;
  }

  const tag = `[${target}]`;
  switch (level) {
    case "error":
      console.error(tag, ...parts);
      break;
    case "warn":
      console.warn(tag, ...parts);
      break;
    case "debug":
    case "trace":
      console.debug(tag, ...parts);
      break;
    default:
      console.log(tag, ...parts);
  }
  pushRing(level, target, message);
  logToBackend(level, target, message);
}

export function log(target: string) {
  return {
    trace: (...parts: unknown[]) => emit("trace", target, ...parts),
    debug: (...parts: unknown[]) => emit("debug", target, ...parts),
    info: (...parts: unknown[]) => emit("info", target, ...parts),
    warn: (...parts: unknown[]) => emit("warn", target, ...parts),
    error: (...parts: unknown[]) => emit("error", target, ...parts),
  };
}

/** Recent UI log lines for the copy-diagnostics export, oldest first. */
export function collectFrontendLog(): string {
  if (ring.length === 0) return "(no ui log entries)";
  return ring
    .map((e) => {
      const ts = new Date(e.t).toISOString().slice(11, 23); // HH:MM:SS.mmm
      return `${ts} ${e.level.toUpperCase().padEnd(5)} [${e.target}] ${e.message}`;
    })
    .join("\n");
}
