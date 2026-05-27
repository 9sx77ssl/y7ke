// Pure formatting helpers — no Tauri, no DOM, no Svelte. Imported by views.

const Y7_PREFIX = "y7:";

/**
 * Truncate a `y7:<base58>` URI to "y7:ABCDEFGH…123456".
 * Returns input unchanged when shorter than the truncation budget.
 */
export function truncateY7Id(y7Id: string, head = 8, tail = 6): string {
  if (!y7Id.startsWith(Y7_PREFIX)) return y7Id;
  const body = y7Id.slice(Y7_PREFIX.length);
  if (body.length <= head + tail + 1) return y7Id;
  return `${Y7_PREFIX}${body.slice(0, head)}…${body.slice(body.length - tail)}`;
}

const HOUR_MS = 60 * 60 * 1000;
const DAY_MS = 24 * HOUR_MS;

/** Format a unix-ms timestamp suitable for message bubbles and contact rows. */
export function formatTimestamp(tsMs: number, now: number = Date.now()): string {
  const t = new Date(tsMs);
  const diff = now - tsMs;
  if (Number.isNaN(t.getTime())) return "";

  const sameDay = isSameLocalDay(tsMs, now);
  if (sameDay) {
    return t.toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  if (diff < 7 * DAY_MS) {
    const weekday = t.toLocaleDateString(undefined, { weekday: "short" });
    const time = t.toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
    });
    return `${weekday} ${time}`;
  }

  return t.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "2-digit",
  });
}

function isSameLocalDay(a: number, b: number): boolean {
  const da = new Date(a);
  const db = new Date(b);
  return (
    da.getFullYear() === db.getFullYear() &&
    da.getMonth() === db.getMonth() &&
    da.getDate() === db.getDate()
  );
}

/** Compact date for sidebar contact rows ("12:04" or "May 22"). */
export function formatRelativeShort(
  tsMs: number,
  now: number = Date.now(),
): string {
  if (isSameLocalDay(tsMs, now)) {
    return new Date(tsMs).toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
    });
  }
  return new Date(tsMs).toLocaleDateString(undefined, {
    month: "short",
    day: "2-digit",
  });
}

/** Map a message status integer to a printable badge. */
export function statusBadge(
  status: 0 | 1 | 2 | 3 | 4,
): { label: string; glyph: string; tone: "muted" | "ok" | "warn" } {
  switch (status) {
    case 0:
      return { label: "Sending", glyph: "…", tone: "muted" };
    case 1:
      return { label: "Sent", glyph: "✓", tone: "muted" };
    case 2:
      return { label: "Delivered", glyph: "✓✓", tone: "ok" };
    case 3:
      return { label: "Synced", glyph: "✓✓", tone: "ok" };
    case 4:
      return { label: "Failed", glyph: "!", tone: "warn" };
  }
}

/** Validate that a string looks like a `y7:<base58>` URI. */
export function isValidY7Uri(input: string): boolean {
  // The Rust side parses with bs58, but we do a cheap shape check before
  // round-tripping to the backend so the UI rejects pasted noise instantly.
  return /^y7:[1-9A-HJ-NP-Za-km-z]{42,46}$/.test(input.trim());
}
