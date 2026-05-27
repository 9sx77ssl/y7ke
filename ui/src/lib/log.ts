// Frontend log helper. Mirrors to console + bridges to backend tracing so
// everything ends up in the Tauri stdout / log file.

import { logToBackend, type LogLevel } from "./bridge";

function emit(level: LogLevel, target: string, ...parts: unknown[]): void {
  const message = parts
    .map((p) => (typeof p === "string" ? p : JSON.stringify(p)))
    .join(" ");
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
