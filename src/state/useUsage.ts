import { create } from "zustand";
import { api, onConfigChanged, onGlassChanged, onUsageUpdated } from "../api";
import type {
  AppView,
  GlassMode,
  ProviderId,
  ProviderView,
  ThemeMode,
  UsageSnapshot,
} from "../types";

interface UsageState {
  view: AppView | null;
  snapshots: Partial<Record<ProviderId, UsageSnapshot>>;
  ready: boolean;
  error: string | null;

  init: () => Promise<void>;
  reload: () => Promise<void>;
  refresh: (provider?: ProviderId) => Promise<void>;
}

export const useUsage = create<UsageState>((set, get) => ({
  view: null,
  snapshots: {},
  ready: false,
  error: null,

  async init() {
    if (!initPromise) {
      initPromise = (async () => {
        await get().reload();

        // Snapshots arrive one provider at a time as each poll lands, rather
        // than in one batch — a slow Anthropic must not hold up a fast DeepSeek.
        await onUsageUpdated((snap) => {
          set((s) => ({ snapshots: { ...s.snapshots, [snap.provider]: snap } }));
        });

        await onGlassChanged((mode) => {
          applyGlass(mode);
          set((s) => (s.view ? { view: { ...s.view, glassMode: mode } } : {}));
        });

        // Settings lives in a separate webview. Rust broadcasts this event so
        // every surface sees provider, appearance and bar changes immediately.
        await onConfigChanged(() => void get().reload());

        watchSystemTheme(() => {
          const view = get().view;
          if (view?.theme === "system") applyTheme(view.theme);
        });
      })().catch((error) => {
        initPromise = null;
        throw error;
      });
    }
    await initPromise;
  },

  async reload() {
    // Config-declared Tauri webviews can start loading just before setup has
    // registered AppState. Retry only that brief startup race; real command
    // failures should still surface immediately.
    for (let attempt = 0; attempt <= 8; attempt += 1) {
      try {
        const view = await api.appView();
        const snapshots: Partial<Record<ProviderId, UsageSnapshot>> = {};
        for (const s of view.snapshots) snapshots[s.provider] = s;
        applyTheme(view.theme);
        applyGlass(view.glassMode);
        set({ view, snapshots, ready: true, error: null });
        return;
      } catch (e) {
        const message = String(e);
        if (attempt < 8 && message.includes("state not managed")) {
          await new Promise((resolve) =>
            window.setTimeout(resolve, 60 * (attempt + 1)),
          );
          continue;
        }
        set({ ready: true, error: message });
        return;
      }
    }
  },

  async refresh(provider) {
    await api.refresh(provider);
  },
}));

// React StrictMode deliberately mounts effects twice in development. Keeping
// one shared initialization promise prevents duplicate native event listeners.
let initPromise: Promise<void> | null = null;

// ---------------------------------------------------------------------------
// Derived selectors
// ---------------------------------------------------------------------------

export function enabledProviders(view: AppView | null): ProviderView[] {
  if (!view) return [];
  return view.providers.filter((p) => p.enabled);
}

/**
 * Cross-provider total.
 *
 * Providers that report no cost at all are simply absent from the sum — the
 * caller is told how many were skipped so the UI can say "3 of 5 reporting"
 * rather than presenting a partial figure as if it were complete.
 */
export function grandTotal(
  view: AppView | null,
  snapshots: Partial<Record<ProviderId, UsageSnapshot>>,
): { cents: number; reporting: number; total: number } {
  const providers = enabledProviders(view);
  let cents = 0;
  let reporting = 0;
  for (const p of providers) {
    const snap = snapshots[p.id];
    if (snap?.costCents !== null && snap?.costCents !== undefined) {
      cents += snap.costCents;
      reporting += 1;
    }
  }
  return { cents, reporting, total: providers.length };
}

export function budgetCentsFor(p: ProviderView): number | null {
  const raw = p.options["budget_usd"];
  if (!raw) return null;
  const n = Number.parseFloat(raw);
  return Number.isFinite(n) ? Math.round(n * 100) : null;
}

// ---------------------------------------------------------------------------
// Theme plumbing
// ---------------------------------------------------------------------------

const darkQuery = () => window.matchMedia("(prefers-color-scheme: dark)");

export function applyTheme(mode: ThemeMode) {
  const dark = mode === "dark" || (mode === "system" && darkQuery().matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

export function applyGlass(mode: GlassMode) {
  document.documentElement.dataset.glass = mode;
}

function watchSystemTheme(cb: () => void) {
  darkQuery().addEventListener("change", cb);
}
