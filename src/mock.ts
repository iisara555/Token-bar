import type {
  AppView,
  AuthMode,
  GlassPref,
  ProviderId,
  ProviderView,
  UsageSnapshot,
} from "./types";

type Unlisten = () => void;

const providerSeed: Array<
  Pick<ProviderView, "id" | "name" | "caps" | "requiredOptions" | "pollSeconds"> &
    Partial<
      Pick<
        ProviderView,
        | "enabled"
        | "hasKey"
        | "needsKey"
        | "fingerprint"
        | "options"
        | "authMode"
        | "usesOauth"
        | "oauthStatus"
      >
    >
> = [
  {
    id: "anthropic",
    name: "Anthropic",
    enabled: true,
    hasKey: false,
    needsKey: true,
    fingerprint: null,
    authMode: "oauth",
    usesOauth: true,
    oauthStatus: "connected",
    caps: { cost: true, tokens: true, balance: false, series: true },
    requiredOptions: [],
    pollSeconds: 900,
    options: { budget_usd: "50" },
  },
  {
    id: "openai",
    name: "OpenAI",
    enabled: true,
    hasKey: false,
    needsKey: true,
    fingerprint: null,
    authMode: "oauth",
    usesOauth: true,
    oauthStatus: "connected",
    caps: { cost: true, tokens: true, balance: false, series: true },
    requiredOptions: [],
    pollSeconds: 900,
    options: { budget_usd: "35" },
  },
  {
    id: "kimi",
    name: "Kimi",
    enabled: false,
    hasKey: false,
    needsKey: true,
    fingerprint: null,
    authMode: "oauth",
    usesOauth: true,
    oauthStatus: "not_found",
    caps: { cost: false, tokens: false, balance: false, series: false },
    requiredOptions: [],
    pollSeconds: 300,
    options: {},
  },
  {
    id: "zai",
    name: "Z.AI",
    caps: { cost: false, tokens: false, balance: false, series: false },
    requiredOptions: [],
    pollSeconds: 300,
  },
  {
    id: "minimax",
    name: "MiniMax",
    caps: { cost: false, tokens: false, balance: false, series: false },
    requiredOptions: [],
    pollSeconds: 300,
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    enabled: true,
    hasKey: true,
    needsKey: true,
    fingerprint: "or-v1-…0d91",
    caps: { cost: false, tokens: false, balance: true, series: false },
    requiredOptions: [],
    pollSeconds: 300,
    options: {},
  },
  {
    id: "xai",
    name: "xAI",
    caps: { cost: false, tokens: false, balance: true, series: false },
    requiredOptions: ["team_id"],
    pollSeconds: 900,
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    caps: { cost: false, tokens: false, balance: true, series: false },
    requiredOptions: [],
    pollSeconds: 300,
  },
  {
    id: "gemini",
    name: "Gemini",
    caps: { cost: false, tokens: false, balance: false, series: false },
    requiredOptions: [],
    pollSeconds: 3600,
  },
  {
    id: "groq",
    name: "Groq",
    caps: { cost: false, tokens: false, balance: false, series: false },
    requiredOptions: [],
    pollSeconds: 3600,
  },
  {
    id: "mistral",
    name: "Mistral",
    caps: { cost: false, tokens: false, balance: false, series: false },
    requiredOptions: [],
    pollSeconds: 3600,
  },
];

const mockLinks: Partial<Record<ProviderId, [string, string]>> = {
  anthropic: ["https://claude.ai/settings", "Link Claude.ai"],
  openai: ["https://chatgpt.com/", "Link ChatGPT"],
  kimi: ["https://www.kimi.com/code", "Link Kimi"],
  zai: ["https://z.ai/subscribe", "Open Z.AI"],
  minimax: ["https://platform.minimaxi.com/subscribe/token-plan", "Open MiniMax"],
  openrouter: ["https://openrouter.ai/settings/keys", "Open OpenRouter"],
  xai: ["https://console.x.ai/", "Open xAI"],
  deepseek: ["https://platform.deepseek.com/api_keys", "Open DeepSeek"],
  gemini: ["https://aistudio.google.com/app/apikey", "Open Google AI"],
  groq: ["https://console.groq.com/keys", "Open Groq"],
  mistral: ["https://console.mistral.ai/api-keys/", "Open Mistral"],
};

const providers: ProviderView[] = providerSeed.map((provider) => ({
  enabled: false,
  needsKey: true,
  hasKey: false,
  fingerprint: null,
  authMode: "api_key",
  usesOauth: false,
  oauthStatus: null,
  linkUrl: mockLinks[provider.id]?.[0] ?? null,
  linkLabel: mockLinks[provider.id]?.[1] ?? null,
  options: {},
  ...provider,
}));

const day = 86_400_000;
const now = Date.now();

const snapshots: UsageSnapshot[] = [
  {
    provider: "anthropic",
    status: "ok",
    costCents: null,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    balanceCents: null,
    limits: {
      fiveHour: {
        usedPercent: 24,
        remainingPercent: 76,
        resetsAt: new Date(now + 2.4 * 60 * 60_000).toISOString(),
        windowMinutes: 300,
      },
      week: {
        usedPercent: 41,
        remainingPercent: 59,
        resetsAt: new Date(now + 4.2 * day).toISOString(),
        windowMinutes: 10_080,
      },
    },
    series: [],
    fetchedAt: new Date(now - 72_000).toISOString(),
    sourceLatencySecs: 3600,
    message: null,
  },
  {
    provider: "openai",
    status: "ok",
    costCents: null,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    balanceCents: null,
    limits: {
      fiveHour: {
        usedPercent: 18,
        remainingPercent: 82,
        resetsAt: new Date(now + 3.1 * 60 * 60_000).toISOString(),
        windowMinutes: 300,
      },
      week: {
        usedPercent: 37,
        remainingPercent: 63,
        resetsAt: new Date(now + 5.7 * day).toISOString(),
        windowMinutes: 10_080,
      },
    },
    series: [],
    fetchedAt: new Date(now - 105_000).toISOString(),
    sourceLatencySecs: 300,
    message: null,
  },
  {
    provider: "openrouter",
    status: "stale",
    costCents: 418,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    balanceCents: 2_582,
    limits: { fiveHour: null, week: null },
    series: [],
    fetchedAt: new Date(now - 4_260_000).toISOString(),
    sourceLatencySecs: 0,
    message: "Showing the last successful balance while OpenRouter retries.",
  },
];

let view: AppView = {
  glass: "auto",
  glassMode: "css",
  windowDays: 30,
  warnAt: 0.8,
  hotkey: "Ctrl+Alt+U",
  clickThrough: false,
  compact: false,
  providers,
  snapshots,
  version: "0.1.0-dev",
};

const storageKey = "token-bar:mock-view-v3";
const autostartStorageKey = "token-bar:mock-autostart";
let mockAutostart = false;
try {
  const saved = window.sessionStorage.getItem(storageKey);
  if (saved) view = JSON.parse(saved) as AppView;
  mockAutostart = window.sessionStorage.getItem(autostartStorageKey) === "true";
} catch {
  // Preview state is disposable. A blocked or corrupt session store should not
  // stop the UI from falling back to the seeded demo data.
}

const usageListeners = new Set<(snapshot: UsageSnapshot) => void>();
const configListeners = new Set<() => void>();
const copy = <T>(value: T): T => structuredClone(value);

function changeProvider(id: ProviderId, update: (provider: ProviderView) => void) {
  const provider = view.providers.find((candidate) => candidate.id === id);
  if (!provider) return;
  update(provider);
  emitConfig();
}

function emitConfig() {
  persist();
  queueMicrotask(() => configListeners.forEach((listener) => listener()));
}

function persist() {
  try {
    window.sessionStorage.setItem(storageKey, JSON.stringify(view));
  } catch {
    // Private browsing and hardened webviews may disable session storage.
  }
}

export const mockApi = {
  appView: async () => copy(view),
  snapshots: async () => copy(view.snapshots),
  noop: async () => undefined,

  async refresh(provider?: ProviderId) {
    const targets = provider
      ? view.snapshots.filter((snapshot) => snapshot.provider === provider)
      : view.snapshots;
    for (const snapshot of targets) {
      snapshot.fetchedAt = new Date().toISOString();
      snapshot.status = "ok";
      snapshot.message = null;
      queueMicrotask(() => usageListeners.forEach((listener) => listener(copy(snapshot))));
    }
    persist();
  },

  async setGlass(glass: GlassPref) {
    view.glass = glass;
    view.glassMode = glass === "off" ? "solid" : "css";
    emitConfig();
  },
  async setWindowDays(days: number) {
    view.windowDays = Math.max(1, Math.min(30, Math.round(days)));
    emitConfig();
  },
  async setProviderEnabled(provider: ProviderId, enabled: boolean) {
    changeProvider(provider, (item) => {
      item.enabled = enabled;
    });
  },
  async setProviderAuthMode(provider: ProviderId, authMode: AuthMode) {
    changeProvider(provider, (item) => {
      item.authMode = authMode;
      item.usesOauth = authMode !== "api_key" && item.oauthStatus !== "not_found";
    });
  },
  async setProviderOption(provider: ProviderId, key: string, value: string) {
    changeProvider(provider, (item) => {
      if (value.trim()) item.options[key] = value.trim();
      else delete item.options[key];
    });
  },
  async saveKey(provider: ProviderId, key: string) {
    const fingerprint = `${key.slice(0, 7)}…${key.slice(-4)}`;
    changeProvider(provider, (item) => {
      item.hasKey = true;
      item.fingerprint = fingerprint;
    });
    return fingerprint;
  },
  async clearKey(provider: ProviderId) {
    changeProvider(provider, (item) => {
      item.hasKey = false;
      item.fingerprint = null;
    });
  },
  async setClickThrough(on: boolean) {
    view.clickThrough = on;
    emitConfig();
  },
  async setCompact(on: boolean) {
    view.compact = on;
    emitConfig();
  },
  async isAutostartEnabled() {
    return mockAutostart;
  },
  async setAutostart(on: boolean) {
    mockAutostart = on;
    try {
      window.sessionStorage.setItem(autostartStorageKey, String(on));
    } catch {
      // Keep the in-memory value when session storage is unavailable.
    }
  },
  async openSettings() {
    window.location.assign("/settings.html");
  },
  async openProviderLink(provider: ProviderId) {
    const url = mockLinks[provider]?.[0];
    if (url) window.open(url, "_blank", "noopener,noreferrer");
  },
};

export function onMockUsageUpdated(listener: (snapshot: UsageSnapshot) => void): Unlisten {
  usageListeners.add(listener);
  return () => usageListeners.delete(listener);
}

export function onMockConfigChanged(listener: () => void): Unlisten {
  configListeners.add(listener);
  return () => configListeners.delete(listener);
}
