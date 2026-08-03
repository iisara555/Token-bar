/** Mirrors the serde output of src-tauri/src/providers/mod.rs. */

export type ProviderId =
  | "anthropic"
  | "openai"
  | "kimi"
  | "zai"
  | "minimax"
  | "openrouter"
  | "xai"
  | "deepseek"
  | "gemini"
  | "groq"
  | "mistral";

export type Status =
  | "ok"
  | "stale"
  | "auth_error"
  | "rate_limited"
  | "unsupported"
  | "not_configured"
  | "error";

export interface TokenBreakdown {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
}

export interface Bucket {
  start: string;
  costCents: number | null;
  tokens: TokenBreakdown;
}

export interface UsageSnapshot {
  provider: ProviderId;
  status: Status;
  costCents: number | null;
  tokens: TokenBreakdown;
  balanceCents: number | null;
  limits: UsageLimits;
  series: Bucket[];
  fetchedAt: string;
  sourceLatencySecs: number;
  message: string | null;
}

export interface QuotaWindow {
  usedPercent: number;
  remainingPercent: number;
  resetsAt: string | null;
  windowMinutes: number;
}

export interface UsageLimits {
  fiveHour: QuotaWindow | null;
  week: QuotaWindow | null;
}

export interface Caps {
  cost: boolean;
  tokens: boolean;
  balance: boolean;
  series: boolean;
}

export interface ProviderView {
  id: ProviderId;
  name: string;
  enabled: boolean;
  authMode: AuthMode;
  usesOauth: boolean;
  oauthStatus: OAuthStatus | null;
  needsKey: boolean;
  hasKey: boolean;
  /** Masked. The key itself never reaches the frontend. */
  fingerprint: string | null;
  caps: Caps;
  requiredOptions: string[];
  options: Record<string, string>;
  pollSeconds: number;
  linkUrl: string | null;
  linkLabel: string | null;
}

export type AuthMode = "auto" | "oauth" | "api_key";
export type OAuthStatus = "connected" | "expired" | "not_found";

export type GlassPref = "auto" | "on" | "off";
/** `native` is DWM Mica/Acrylic; `vibrancy` is an AppKit NSVisualEffectView. */
export type GlassMode = "native" | "vibrancy" | "css" | "solid";

export interface AppView {
  glass: GlassPref;
  glassMode: GlassMode;
  windowDays: number;
  warnAt: number;
  hotkey: string;
  clickThrough: boolean;
  compact: boolean;
  providers: ProviderView[];
  snapshots: UsageSnapshot[];
  version: string;
}

export const PROVIDER_ORDER: ProviderId[] = [
  "anthropic",
  "openai",
  "kimi",
  "zai",
  "minimax",
  "openrouter",
  "xai",
  "deepseek",
  "gemini",
  "groq",
  "mistral",
];

