import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  disable as disableAutostart,
  enable as enableAutostart,
  isEnabled as isAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import { mockApi, onMockConfigChanged, onMockUsageUpdated } from "./mock";
import type {
  AppView,
  AuthMode,
  GlassMode,
  GlassPref,
  ProviderId,
  ThemeMode,
  UsageSnapshot,
} from "./types";

export const EVENT_UPDATED = "usage://updated";
export const EVENT_GLASS = "glass://changed";
export const EVENT_CONFIG = "config://changed";

function run<T>(
  command: string,
  args: Record<string, unknown> | undefined,
  fallback: () => T | Promise<T>,
): Promise<T> {
  return isTauri() ? invoke<T>(command, args) : Promise.resolve().then(fallback);
}

export const api = {
  appView: () => run<AppView>("get_app_view", undefined, mockApi.appView),
  snapshots: () => run<UsageSnapshot[]>("get_snapshots", undefined, mockApi.snapshots),
  refresh: (provider?: ProviderId) =>
    run<void>("refresh", { provider: provider ?? null }, () => mockApi.refresh(provider)),

  setTheme: (theme: ThemeMode) =>
    run<void>("set_theme", { theme }, () => mockApi.setTheme(theme)),
  setGlass: (glass: GlassPref) =>
    run<void>("set_glass", { glass }, () => mockApi.setGlass(glass)),
  setWindowDays: (days: number) =>
    run<void>("set_window_days", { days }, () => mockApi.setWindowDays(days)),
  setWarnAt: (fraction: number) =>
    run<void>("set_warn_at", { fraction }, () => mockApi.setWarnAt(fraction)),
  setHotkey: (accelerator: string) =>
    run<void>("set_hotkey", { accelerator }, () => mockApi.setHotkey(accelerator)),

  setProviderEnabled: (provider: ProviderId, enabled: boolean) =>
    run<void>("set_provider_enabled", { provider, enabled }, () =>
      mockApi.setProviderEnabled(provider, enabled),
    ),
  setProviderAuthMode: (provider: ProviderId, authMode: AuthMode) =>
    run<void>("set_provider_auth_mode", { provider, authMode }, () =>
      mockApi.setProviderAuthMode(provider, authMode),
    ),
  setProviderOption: (provider: ProviderId, key: string, value: string) =>
    run<void>("set_provider_option", { provider, key, value }, () =>
      mockApi.setProviderOption(provider, key, value),
    ),

  /** Returns the masked fingerprint. The key stays in the credential store. */
  saveKey: (provider: ProviderId, key: string) =>
    run<string>("save_provider_key", { provider, key }, () => mockApi.saveKey(provider, key)),
  clearKey: (provider: ProviderId) =>
    run<void>("clear_provider_key", { provider }, () => mockApi.clearKey(provider)),

  barSetSize: (width: number, height: number) =>
    run<void>("bar_set_size", { width, height }, mockApi.noop),
  barDropped: () => run<void>("bar_dropped", undefined, mockApi.noop),
  setClickThrough: (on: boolean) =>
    run<void>("set_click_through", { on }, () => mockApi.setClickThrough(on)),
  setCompact: (on: boolean) =>
    run<void>("set_compact", { on }, () => mockApi.setCompact(on)),
  setAllowInNotch: (on: boolean) =>
    run<void>("set_allow_in_notch", { on }, () => mockApi.setAllowInNotch(on)),
  isAutostartEnabled: () =>
    isTauri() ? isAutostartEnabled() : mockApi.isAutostartEnabled(),
  setAutostart: (on: boolean) =>
    isTauri() ? (on ? enableAutostart() : disableAutostart()) : mockApi.setAutostart(on),

  openSettings: () => run<void>("open_settings", undefined, mockApi.openSettings),
  openProviderLink: (provider: ProviderId) =>
    run<void>("open_provider_link", { provider }, () => mockApi.openProviderLink(provider)),
  hideWindow: (label: string) =>
    run<void>("hide_window", { label }, mockApi.noop),
  quit: () => run<void>("quit", undefined, mockApi.noop),
};

export function onUsageUpdated(
  handler: (snap: UsageSnapshot) => void,
): Promise<UnlistenFn> {
  return isTauri()
    ? listen<UsageSnapshot>(EVENT_UPDATED, (e) => handler(e.payload))
    : Promise.resolve(onMockUsageUpdated(handler));
}

export function onGlassChanged(handler: (mode: GlassMode) => void): Promise<UnlistenFn> {
  return isTauri()
    ? listen<GlassMode>(EVENT_GLASS, (e) => handler(e.payload))
    : Promise.resolve(() => undefined);
}

export function onConfigChanged(handler: () => void): Promise<UnlistenFn> {
  return isTauri()
    ? listen(EVENT_CONFIG, handler)
    : Promise.resolve(onMockConfigChanged(handler));
}
