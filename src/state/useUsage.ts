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

/**
 * How much of something finite is already spent, across everything enabled.
 *
 * The bar shows many numbers in many units — dollars, tokens, percentages of
 * two unrelated rate-limit windows. None of them can be summed. What they do
 * share is a shape: each one is a fraction of a ceiling that resets. This
 * reduces all of them to that fraction and reports the **worst** one.
 *
 * Worst, not average. Averaging is what turns "the weekly limit is 4% from
 * gone" into a reassuring 30%, and the entire point of an ambient rail is to
 * be the thing that was already showing red before anyone thought to look.
 *
 * Returns null when nothing enabled has a ceiling to measure against — no
 * budgets set and no provider reporting quota windows. That is genuinely
 * different from zero pressure and the rail says so differently.
 */
export function guardLoad(
  view: AppView | null,
  snapshots: Partial<Record<ProviderId, UsageSnapshot>>,
): GuardLoad | null {
  let worst: GuardLoad | null = null;

  const consider = (ratio: number, provider: ProviderView, reason: string) => {
    if (!Number.isFinite(ratio)) return;
    const clamped = Math.max(0, ratio);
    if (worst === null || clamped > worst.ratio) {
      worst = { ratio: clamped, provider: provider.name, reason };
    }
  };

  for (const p of enabledProviders(view)) {
    const snap = snapshots[p.id];
    if (!snap) continue;

    const budget = budgetCentsFor(p);
    if (budget !== null && budget > 0 && snap.costCents !== null) {
      consider(snap.costCents / budget, p, "budget");
    }

    // A quota window reports what is *left*; the rail reads what is gone.
    const { fiveHour, week } = snap.limits ?? {};
    if (fiveHour) consider(1 - fiveHour.remainingPercent / 100, p, "5-hour limit");
    if (week) consider(1 - week.remainingPercent / 100, p, "weekly limit");
  }

  return worst;
}

export interface GuardLoad {
  /** 0 at rest, 1 at the ceiling. Can exceed 1 — a blown budget is not capped. */
  ratio: number;
  /** Whose ceiling this is, for the label. */
  provider: string;
  /** Which ceiling: "budget", "5-hour limit", "weekly limit". */
  reason: string;
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
