// "Copy diagnostics" payload. Assembles a single copy-pasteable snapshot
// from the same backend reads the Connectivity pane uses, plus the
// rate-limited frontend log ring buffer. This is where the verbose detail
// (per-peer transport, via-host, bootstrap RTT, recent UI events) lives —
// the permanent UI stays compact; the debug surface is the export.

import { getVersion } from "@tauri-apps/api/app";

import {
  getDcutrStats,
  getDiagnosticsDetail,
  getNatStatus,
  getSettings,
  listActiveConnections,
  listBootstraps,
} from "./bridge";
import type { ConnectionView } from "./gen/ConnectionView";
import { collectFrontendLog } from "./log";

function transportLabel(c: ConnectionView): string {
  const fam = c.ip_version === "v6" ? "ipv6" : c.ip_version === "v4" ? "ipv4" : null;
  const base =
    c.kind === "relayed"
      ? "relayed"
      : `${c.kind}${fam ? ` / ${fam}` : ""} / ${c.transport ? c.transport.toLowerCase() : "?"}`;
  // Origin = the "how did we get here?" axis (public_ipv6 / dcutr_upgrade / …).
  return c.origin && c.origin !== "unknown" ? `${base}  [${c.origin}]` : base;
}

/** Friendly dial-mode label (matches the Connectivity pane wording). */
function dialModeLabel(m: string | undefined): string {
  if (m === "LanOnly") return "lan only";
  if (m === "Internet") return "Y7net";
  return "—";
}

/** Build the full diagnostics text blob. */
export async function buildDiagnostics(): Promise<string> {
  let version = "unknown";
  try {
    version = await getVersion();
  } catch {
    /* version is best-effort */
  }

  const [nat, dcutr, conns, boots, detail, settings] = await Promise.all([
    getNatStatus(),
    getDcutrStats(),
    listActiveConnections(),
    listBootstraps(),
    getDiagnosticsDetail(),
    getSettings(),
  ]);

  const att = Number(dcutr.attempts);
  const suc = Number(dcutr.successes);
  const fail = Number(dcutr.failures);
  const rate = att > 0 ? `${Math.round((suc / att) * 100)}%` : "n/a";
  // Relay fallback is "active" when a peer is reachable only via relay.
  const relayActive =
    conns.some((c) => c.kind === "relayed") &&
    !conns.some((c) => c.kind === "direct");

  const L: string[] = [];
  L.push(`y7ke diagnostics — ${new Date().toISOString()}`);
  L.push(`version:        ${version}`);
  L.push(`dial mode:      ${dialModeLabel(settings.dial_mode)}`);
  L.push(`nat status:     ${nat}`);
  const nd = detail.nat_detail;
  if (nd.last_tested_addr || nd.consecutive_failures > 0) {
    L.push(
      `  nat probe:    ${nd.last_tested_addr ?? "—"}  fails=${nd.consecutive_failures}${nd.last_probe_server ? `  via ${nd.last_probe_server}` : ""}`,
    );
  }
  L.push(`dcutr:          ${suc}/${att} (${rate})  failures=${fail}`);
  for (const r of detail.recent_dcutr_failures) {
    L.push(`  dcutr fail:   ${r}`);
  }
  const rl = detail.rate_limit_drops;
  const rlTotal = Number(rl.handshake) + Number(rl.msg) + Number(rl.sync);
  if (rlTotal > 0) {
    L.push(
      `rate-limit drops: hs=${Number(rl.handshake)} msg=${Number(rl.msg)} sync=${Number(rl.sync)}`,
    );
  }
  L.push(`relay fallback: ${relayActive ? "active" : "no"}`);
  L.push("");
  L.push(`bootstraps (${boots.length}):`);
  for (const b of boots) {
    const ping = b.last_ping_failed
      ? "unreachable"
      : b.last_ping_ms !== null
        ? `${Number(b.last_ping_ms)}ms`
        : "—";
    L.push(`  ${b.multiaddr}  ping=${ping}${b.is_default ? "  [default]" : ""}`);
  }
  L.push("");
  L.push(`active connections (${conns.length}):`);
  for (const c of conns) {
    L.push(
      `  ${c.y7_id}  ${transportLabel(c)}${c.via_host ? `  via ${c.via_host}` : ""}`,
    );
  }
  L.push("");
  L.push("--- ui log ---");
  L.push(collectFrontendLog());
  return L.join("\n");
}

/** Build + copy to clipboard. Returns the text (for logging). */
export async function copyDiagnostics(): Promise<void> {
  const text = await buildDiagnostics();
  await navigator.clipboard.writeText(text);
}
