import type { Status, TokenBreakdown } from "./types";

/**
 * Money, rendered at the precision that is actually meaningful.
 *
 * Sub-cent API spend is common on a quiet day, and rounding it to "$0.00" makes
 * a working integration look broken, so small amounts keep more decimals.
 */
export function money(cents: number | null | undefined): string {
  if (cents === null || cents === undefined) return "—";
  const dollars = cents / 100;
  if (dollars === 0) return "$0.00";
  if (Math.abs(dollars) < 0.01) return `$${dollars.toFixed(4)}`;
  if (Math.abs(dollars) < 1000) return `$${dollars.toFixed(2)}`;
  return `$${Math.round(dollars).toLocaleString("en-US")}`;
}

/** Compact token counts: 1.2M reads faster than 1,240,000 on a 48px chip. */
export function tokens(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}K`;
  if (n < 1_000_000_000) return `${(n / 1_000_000).toFixed(n < 10_000_000 ? 1 : 0)}M`;
  return `${(n / 1_000_000_000).toFixed(1)}B`;
}

export function totalTokens(t: TokenBreakdown): number {
  return t.input + t.output + t.cacheRead + t.cacheWrite;
}

export function relativeTime(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "never";
  const secs = Math.max(0, Math.round((Date.now() - then) / 1000));
  if (secs < 45) return "just now";
  if (secs < 3600) return `${Math.round(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.round(secs / 3600)}h ago`;
  return `${Math.round(secs / 86400)}d ago`;
}

/** Human-readable countdown for rate-limit windows. */
export function resetIn(iso: string | null | undefined): string {
  if (!iso) return "reset time unavailable";
  const millis = new Date(iso).getTime() - Date.now();
  if (Number.isNaN(millis)) return "reset time unavailable";
  if (millis <= 0) return "resetting now";
  const minutes = Math.ceil(millis / 60_000);
  if (minutes < 60) return `resets in ${minutes}m`;
  const hours = Math.ceil(minutes / 60);
  if (hours < 48) return `resets in ${hours}h`;
  return `resets in ${Math.ceil(hours / 24)}d`;
}

/** Compact countdown used inside the always-on-top bar. */
export function resetInShort(iso: string | null | undefined): string {
  if (!iso) return "—";
  const millis = new Date(iso).getTime() - Date.now();
  if (Number.isNaN(millis)) return "—";
  if (millis <= 0) return "now";
  const minutes = Math.ceil(millis / 60_000);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.ceil(minutes / 60);
  if (hours < 48) return `${hours}h`;
  return `${Math.ceil(hours / 24)}d`;
}

/** Absolute local time shown beside the countdown in the detail card. */
export function resetAt(iso: string | null | undefined): string {
  if (!iso) return "reset time unavailable";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "reset time unavailable";
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

const STATUS_LABEL: Record<Status, string> = {
  ok: "Live",
  stale: "Stale",
  auth_error: "Key rejected",
  rate_limited: "Rate limited",
  unsupported: "No API",
  not_configured: "No key",
  error: "Error",
};

export function statusLabel(s: Status): string {
  return STATUS_LABEL[s] ?? s;
}

export function statusTone(s: Status): "ok" | "warn" | "danger" | "neutral" {
  switch (s) {
    case "ok":
      return "ok";
    case "stale":
    case "rate_limited":
      return "warn";
    case "auth_error":
    case "error":
      return "danger";
    default:
      return "neutral";
  }
}

/**
 * A Tauri accelerator, written the way the platform writes shortcuts.
 *
 * The stored value is `CmdOrControl+Alt+U` — correct as configuration, and
 * meaningless as a label. Windows spells the same shortcut `Ctrl+Alt+U`; macOS
 * spells it `⌥⌘U`, with no separators and with the modifiers in the fixed order
 * Apple uses everywhere (⌃⌥⇧⌘), because a Mac user reads that as one glyph
 * cluster rather than as a sentence to parse.
 */
export function hotkeyLabel(accelerator: string, mac: boolean): string {
  const parts = accelerator.split("+").map((p) => p.trim()).filter(Boolean);

  if (!mac) {
    return parts
      .map((p) => (/^(cmdorcontrol|commandorcontrol|cmdorctrl)$/i.test(p) ? "Ctrl" : p))
      .join("+");
  }

  const SYMBOL: Record<string, string> = {
    cmdorcontrol: "⌘",
    commandorcontrol: "⌘",
    cmdorctrl: "⌘",
    cmd: "⌘",
    command: "⌘",
    super: "⌘",
    ctrl: "⌃",
    control: "⌃",
    alt: "⌥",
    option: "⌥",
    shift: "⇧",
  };
  // Apple's canonical modifier order. Anything unrecognised is a key, not a
  // modifier, and sorts to the end where the key belongs.
  const ORDER = ["⌃", "⌥", "⇧", "⌘"];

  const symbols: string[] = [];
  const keys: string[] = [];
  for (const part of parts) {
    const symbol = SYMBOL[part.toLowerCase()];
    if (symbol) symbols.push(symbol);
    else keys.push(part.length === 1 ? part.toUpperCase() : part);
  }

  symbols.sort((a, b) => ORDER.indexOf(a) - ORDER.indexOf(b));
  return [...symbols, ...keys].join("");
}
