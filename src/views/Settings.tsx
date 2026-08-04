import { useEffect, useState } from "react";
import { api } from "../api";
import { useUsage } from "../state/useUsage";
import {
  type AuthMode,
  type GlassPref,
  type ProviderId,
  type ProviderView,
} from "../types";
import { hotkeyLabel } from "../format";
import { CloseIcon, KeyIcon } from "../components/Icons";
import { ProviderLogo } from "../components/ProviderLogo";
import { detectOs } from "../entries/boot";
import { useWindowFit } from "../state/useWindowFit";

/** What this desktop calls its own light/dark setting, for the hint copy. */
const OS_NAMES: Record<ReturnType<typeof detectOs>, string> = {
  macos: "macOS",
  windows: "Windows",
  linux: "your desktop",
};

export function Settings() {
  const { view, ready, init, reload } = useUsage();
  const [autostart, setAutostart] = useState(false);
  const [autostartReady, setAutostartReady] = useState(false);
  const os = detectOs();
  const osName = OS_NAMES[os];
  const autostartLabel = os === "macos" ? "Open at login" : `Start with ${osName}`;
  // Naming the actual store matters here: this paragraph is the app's claim
  // about where a user's admin keys end up, and "Windows Credential Manager" on
  // a Mac is a false one.
  const credentialStoreName =
    os === "macos" ? "macOS Keychain" : os === "windows" ? "Windows Credential Manager" : "system credential store";

  // Matches the `settings` window in tauri.conf.json.
  useWindowFit("settings", { width: 760, height: 620 });

  useEffect(() => {
    void init();
  }, [init]);

  // A frameless window still owes the user the two ways every window on their
  // machine closes. Esc is the Windows habit, Cmd-W the Mac one; neither costs
  // anything to honour on the other platform.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const close =
        e.key === "Escape" || ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "w");
      if (!close) return;
      e.preventDefault();
      void api.hideWindow("settings");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    void api
      .isAutostartEnabled()
      .then((enabled) => setAutostart(enabled))
      .catch(() => setAutostart(false))
      .finally(() => setAutostartReady(true));
  }, []);

  if (!ready || !view) {
    return (
      <div className="settings-root">
        <div className="glass settings">
          <div className="empty">Loading…</div>
        </div>
      </div>
    );
  }

  return (
    <div className="settings-root">
      <div className="glass settings">
        <div className="titlebar" data-tauri-drag-region>
          <h1 data-tauri-drag-region>Token Bar</h1>
          <button
            type="button"
            className="icon-btn"
            onClick={() => void api.hideWindow("settings")}
            title="Close"
          >
            <CloseIcon />
          </button>
        </div>

        <div className="settings-body">
          <section>
            <h2 className="section-title">Appearance</h2>
            <div className="card">
              <div className="field">
                <div>
                  <div className="field-label">Glass</div>
                  <div className="field-hint">
                    Currently rendering in <strong>{view.glassMode}</strong> mode.
                    {view.glassMode === "solid" &&
                      (os === "macos"
                        ? " Reduce transparency is on in Accessibility settings, or you turned glass off here."
                        : ` ${osName} transparency effects are off, or you turned glass off here.`)}
                  </div>
                </div>
                <Segmented<GlassPref>
                  value={view.glass}
                  options={[
                    ["auto", "Auto"],
                    ["on", "On"],
                    ["off", "Off"],
                  ]}
                  onChange={async (v) => {
                    await api.setGlass(v);
                    await reload();
                  }}
                />
              </div>

              <div className="field">
                <div>
                  <div className="field-label">Click-through</div>
                  <div className="field-hint">
                    The bar stays visible but stops catching mouse clicks. Use the{" "}
                    <code>{hotkeyLabel(view.hotkey, os === "macos")}</code> hotkey to hide it
                    entirely.
                  </div>
                </div>
                <Switch
                  label="Click-through"
                  checked={view.clickThrough}
                  onChange={async (v) => {
                    await api.setClickThrough(v);
                    await reload();
                  }}
                />
              </div>

              <div className="field">
                <div>
                  <div className="field-label">Compact bar</div>
                  <div className="field-hint">
                    Shorten the meters and drop the wordmark and reset times. Every
                    reading stays — only the room around them goes.
                  </div>
                </div>
                <Switch
                  label="Compact bar"
                  checked={view.compact}
                  onChange={async (v) => {
                    await api.setCompact(v);
                    await reload();
                  }}
                />
              </div>

              <div className="field">
                <div>
                  {/* macOS calls this "Open at Login" everywhere it appears, and
                      the setting genuinely is a Login Item there rather than a
                      Run key. Borrowing the platform's own words is the whole
                      difference between a setting someone recognises and one
                      they have to reason about. */}
                  <div className="field-label">{autostartLabel}</div>
                  <div className="field-hint">
                    Launch Token Bar automatically after you sign in.
                  </div>
                </div>
                <Switch
                  label={autostartLabel}
                  checked={autostart}
                  disabled={!autostartReady}
                  onChange={async (v) => {
                    await api.setAutostart(v);
                    setAutostart(v);
                  }}
                />
              </div>

              {os === "macos" && (
                <div className="field">
                  <div>
                    <div className="field-label">Allow in the menu bar</div>
                    <div className="field-hint">
                      Let the bar sit up in the menu bar strip. On a Mac with a
                      camera housing it keeps itself to one side of the notch
                      rather than hiding behind it.
                    </div>
                  </div>
                  <Switch
                    label="Allow the bar in the menu bar"
                    checked={view.allowInNotch}
                    onChange={async (v) => {
                      await api.setAllowInNotch(v);
                      await reload();
                    }}
                  />
                </div>
              )}

              <div className="field">
                <div>
                  <div className="field-label">Shortcut</div>
                  <div className="field-hint">
                    Shows and hides the bar. Press a combination with at least one
                    modifier.
                  </div>
                </div>
                <HotkeyField
                  accelerator={view.hotkey}
                  mac={os === "macos"}
                  onChange={async (next) => {
                    await api.setHotkey(next);
                    await reload();
                  }}
                />
              </div>

              <div className="field">
                <div>
                  <div className="field-label">Warn at</div>
                  <div className="field-hint">
                    How full the tightest budget or rate-limit window gets before the
                    rail and the tray icon start warning.
                  </div>
                </div>
                <Segmented<string>
                  value={String(Math.round(view.warnAt * 100))}
                  options={[
                    ["60", "60%"],
                    ["70", "70%"],
                    ["80", "80%"],
                    ["90", "90%"],
                  ]}
                  onChange={async (v) => {
                    await api.setWarnAt(Number(v) / 100);
                    await reload();
                  }}
                />
              </div>

              <div className="field">
                <div>
                  <div className="field-label">Reporting window</div>
                  <div className="field-hint">
                    Days of history to total up. Capped at 30 — Anthropic&rsquo;s daily
                    buckets do not go further back in one request.
                  </div>
                </div>
                <input
                  type="number"
                  min={1}
                  max={30}
                  aria-label="Reporting window in days"
                  defaultValue={view.windowDays}
                  style={{ width: 72 }}
                  onBlur={async (e) => {
                    await api.setWindowDays(Number(e.currentTarget.value) || 30);
                    await reload();
                  }}
                />
              </div>
            </div>
          </section>

          <section>
            <h2 className="section-title">Providers</h2>
            <div className="card">
              {view.providers.map((p) => (
                <ProviderRow key={p.id} provider={p} onChanged={reload} />
              ))}
            </div>
          </section>

          <section>
            <h2 className="section-title">Privacy &amp; credentials</h2>
            <div className="card">
              <p
                style={{
                  fontSize: "var(--text-sm)",
                  lineHeight: 1.55,
                  margin: 0,
                  color: "var(--text-dim)",
                }}
              >
                OAuth reads the existing <strong>Claude Code, Codex or Kimi Code login</strong>{" "}
                directly each time. Expired Claude Code access tokens are refreshed through
                Anthropic and the rotated pair is saved back to that official credentials file.
                Antigravity has no existing login to read, so its Connect button runs a Google
                sign-in of Token Bar&rsquo;s own; the resulting token is kept apart from API keys,
                under its own entry in the <strong>{credentialStoreName}</strong>. The Link
                buttons open the provider page in Chrome; Token Bar never reads or copies Chrome
                cookies. API keys are stored in the <strong>{credentialStoreName}</strong> under{" "}
                <code>com.tokenbar.app</code>, never in a config file and never in this window —
                once saved, only the Rust process can read them, and it sends them nowhere but the
                provider&rsquo;s own API. Removing a key here deletes it from the credential
                store.
              </p>
            </div>
          </section>

          <UpdateFooter version={view.version} />
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------

/**
 * The version line, and the button that asks whether it is still the newest.
 *
 * Checking is a button rather than something that happens on every launch. This
 * app's whole premise is that it does not spend the user's attention without a
 * reading to show for it, and a background check that phones GitHub on startup
 * spends network and a request quota to usually learn nothing.
 *
 * It reports; it does not install. Installing in place means downloading and
 * running a binary, and doing that safely needs a signed update artifact — so
 * until the signing key exists, the honest end of this flow is the releases
 * page, where the user can see what they are downloading.
 */
function UpdateFooter({ version }: { version: string }) {
  const [state, setState] = useState<
    | { kind: "idle" }
    | { kind: "checking" }
    | { kind: "current" }
    | { kind: "available"; latest: string }
    | { kind: "failed"; message: string }
  >({ kind: "idle" });

  async function check() {
    setState({ kind: "checking" });
    try {
      const result = await api.checkForUpdate();
      if (result.available && result.latest) {
        setState({ kind: "available", latest: result.latest });
      } else {
        setState({ kind: "current" });
      }
    } catch (e) {
      // Offline is the ordinary reason this fails, and it is not the user's
      // problem to solve — say what happened and leave the button usable.
      setState({ kind: "failed", message: String(e) });
    }
  }

  return (
    <div className="update-foot">
      <span className="faint">Token Bar {version}</span>

      {state.kind === "available" ? (
        <button
          type="button"
          className="btn"
          data-accent
          onClick={() => void api.openReleasesPage()}
        >
          Download {state.latest}
        </button>
      ) : (
        <button
          type="button"
          className="btn"
          onClick={() => void check()}
          disabled={state.kind === "checking"}
        >
          {state.kind === "checking" ? "Checking…" : "Check for updates"}
        </button>
      )}

      {state.kind === "current" && <span className="faint">Up to date.</span>}
      {state.kind === "failed" && (
        <span className="faint">Could not reach GitHub.</span>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------

function ProviderRow({
  provider,
  onChanged,
}: {
  provider: ProviderView;
  onChanged: () => Promise<void>;
}) {
  const [open, setOpen] = useState(false);
  const [keyDraft, setKeyDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [connecting, setConnecting] = useState(false);

  const saveKey = async () => {
    if (!keyDraft.trim()) return;
    try {
      await api.saveKey(provider.id, keyDraft);
      setKeyDraft("");
      setError(null);
      await onChanged();
    } catch (e) {
      setError(String(e));
    }
  };

  const connectAntigravity = async () => {
    setConnecting(true);
    setError(null);
    try {
      await api.antigravityLogin();
      await onChanged();
    } catch (e) {
      setError(String(e));
    } finally {
      setConnecting(false);
    }
  };

  const disconnectAntigravity = async () => {
    try {
      await api.antigravityLogout();
      setError(null);
      await onChanged();
    } catch (e) {
      setError(String(e));
    }
  };

  const setOption = async (key: string, value: string) => {
    try {
      await api.setProviderOption(provider.id, key, value);
      setError(null);
      await onChanged();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <>
      <div className="provider-row">
        <span className="chip-mark">
          <ProviderLogo provider={provider.id} />
        </span>
        <button
          type="button"
          style={{ textAlign: "left" }}
          onClick={() => setOpen(!open)}
          aria-expanded={open}
        >
          <div style={{ fontSize: 13, fontWeight: 600 }}>{provider.name}</div>
          <div className="field-hint" style={{ marginTop: 1 }}>
            {describe(provider)}
          </div>
        </button>
        <Switch
          label={`${provider.name} enabled`}
          checked={provider.enabled}
          onChange={async (v) => {
            await api.setProviderEnabled(provider.id, v);
            await onChanged();
          }}
        />
        {provider.linkUrl && (
          <button
            type="button"
            className="btn provider-link"
            title={`Open ${provider.name} in Chrome`}
            onClick={(event) => {
              event.stopPropagation();
              void api.openProviderLink(provider.id).catch((e) => setError(String(e)));
            }}
          >
            {provider.linkLabel ?? "Link"}
          </button>
        )}
      </div>

      {open && (
        <div className="provider-detail">
          {provider.oauthStatus !== null && provider.id !== "antigravity" && (
            <div className="auth-choice">
              <div>
                <div className="field-label">Authentication</div>
                <div className="field-hint">Use a subscription login or an admin API key.</div>
              </div>
              <Segmented<AuthMode>
                value={provider.authMode}
                options={[
                  ["auto", "Auto"],
                  ["oauth", "OAuth"],
                  ["api_key", "API key"],
                ]}
                onChange={async (authMode) => {
                  await api.setProviderAuthMode(provider.id, authMode);
                  await onChanged();
                }}
              />
            </div>
          )}

          {provider.usesOauth && (
            <div className="oauth-status" data-status={provider.oauthStatus}>
              <strong>{oauthStatusTitle(provider)}</strong>
              <span>{oauthStatusHelp(provider)}</span>
            </div>
          )}

          {provider.id === "anthropic" && provider.linkUrl && (
            <div className="notice">
              Link opens Claude.ai in Chrome. A Chrome login is not copied into Token Bar; live
              quota data comes from the official OAuth session file owned by Claude Code.
            </div>
          )}
          {provider.id === "kimi" && provider.linkUrl && (
            <div className="notice">
              Link opens Kimi Code in Chrome. After signing in with Kimi Code, choose OAuth here
              and refresh to read the five-hour and weekly plan windows.
            </div>
          )}

          {provider.id === "antigravity" && (
            <div className="field">
              <div>
                <div className="field-label">Google account</div>
                <div className="field-hint">
                  Connect opens a Google sign-in in your browser. This is Token Bar&rsquo;s own
                  login, separate from a Gemini API key and from Antigravity&rsquo;s own login on
                  this PC — the token is kept in the{" "}
                  {detectOs() === "macos" ? "macOS Keychain" : "system credential store"} and used
                  only to read your quota. Antigravity publishes no usage API, so the reading
                  comes from the same undocumented endpoint Antigravity itself calls and may break
                  if Google changes it.
                </div>
              </div>
              {provider.oauthStatus === "connected" ? (
                <button
                  type="button"
                  className="btn"
                  data-tone="danger"
                  onClick={() => void disconnectAntigravity()}
                >
                  Disconnect
                </button>
              ) : (
                <button
                  type="button"
                  className="btn"
                  disabled={connecting}
                  onClick={() => void connectAntigravity()}
                >
                  {connecting ? "Waiting for Google…" : "Connect Google account"}
                </button>
              )}
            </div>
          )}

          {provider.needsKey && !provider.usesOauth && (
            <label>
              <KeyIcon />
              {provider.hasKey ? (
                <>
                  <code style={{ fontSize: 11.5 }}>{provider.fingerprint}</code>
                  <button
                    type="button"
                    className="btn"
                    data-tone="danger"
                    onClick={async () => {
                      await api.clearKey(provider.id);
                      await onChanged();
                    }}
                  >
                    Remove
                  </button>
                </>
              ) : (
                <>
                  <input
                    type="password"
                    placeholder={placeholderFor(provider.id)}
                    value={keyDraft}
                    autoComplete="off"
                    spellCheck={false}
                    onChange={(e) => setKeyDraft(e.currentTarget.value)}
                    onKeyDown={(e) => e.key === "Enter" && void saveKey()}
                  />
                  <button type="button" className="btn" onClick={() => void saveKey()}>
                    Save
                  </button>
                </>
              )}
            </label>
          )}

          {provider.requiredOptions.map((opt) => (
            <label key={opt}>
              <span style={{ minWidth: 74 }}>{opt.replace(/_/g, " ")}</span>
              <input
                type="text"
                defaultValue={provider.options[opt] ?? ""}
                placeholder="required"
                onBlur={(e) => void setOption(opt, e.currentTarget.value)}
              />
            </label>
          ))}

          {!provider.usesOauth && <label>
            <span style={{ minWidth: 74 }}>Budget USD</span>
            <input
              type="text"
              inputMode="decimal"
              defaultValue={provider.options["budget_usd"] ?? ""}
              placeholder="optional, e.g. 50"
              onBlur={(e) => void setOption("budget_usd", e.currentTarget.value)}
            />
          </label>}

          {provider.manualEntry && (
            <label>
              <span style={{ minWidth: 74 }}>Spend USD</span>
              <input
                type="text"
                inputMode="decimal"
                defaultValue={provider.options["manual_spend_usd"] ?? ""}
                placeholder="entered by hand — no API available"
                onBlur={(e) => void setOption("manual_spend_usd", e.currentTarget.value)}
              />
            </label>
          )}

          {error && (
            <div className="notice" data-tone="danger">
              {error}
            </div>
          )}
        </div>
      )}
    </>
  );
}

function describe(p: ProviderView): string {
  if (p.id === "kimi") {
    return `Kimi Code OAuth/API key · 5-hour + weekly limits · every ${pollEvery(p)}`;
  }
  if (p.id === "antigravity") {
    return `Google OAuth · model quota, reverse-engineered · every ${pollEvery(p)}`;
  }
  if (p.id === "zai" || p.id === "minimax") {
    return `Plan quota · 5-hour + weekly limits · every ${pollEvery(p)}`;
  }
  if (p.usesOauth) {
    return `OAuth subscription · 5-hour + weekly limits · every ${pollEvery(p)}`;
  }
  if (!p.caps.cost && !p.caps.balance) return "No usage API — manual entry only";
  const parts: string[] = [];
  if (p.caps.cost) parts.push("cost");
  if (p.caps.tokens) parts.push("tokens");
  if (p.caps.balance) parts.push("balance");
  return `${parts.join(" · ")} · every ${pollEvery(p)}`;
}

function pollEvery(p: ProviderView): string {
  return p.pollSeconds >= 3600
    ? `${Math.round(p.pollSeconds / 3600)}h`
    : `${Math.round(p.pollSeconds / 60)}m`;
}

function oauthStatusTitle(p: ProviderView): string {
  if (p.oauthStatus === "connected") return "OAuth connected";
  if (p.oauthStatus === "expired") return "OAuth session expired";
  return "OAuth login not found";
}

function oauthStatusHelp(p: ProviderView): string {
  if (p.id === "antigravity") {
    return p.oauthStatus === "connected"
      ? "Connected with Token Bar's own Google sign-in."
      : "Use Connect below to sign in again.";
  }
  const client = p.id === "anthropic" ? "Claude Code" : p.id === "kimi" ? "Kimi Code" : "Codex";
  if (p.oauthStatus === "connected") {
    return `Using the existing ${client} login on this PC. The token remains owned by ${client}.`;
  }
  return `Sign in again with ${client}, then press Refresh in Token Bar.`;
}

function placeholderFor(id: ProviderId): string {
  switch (id) {
    case "anthropic":
      return "sk-ant-admin01-… (Admin key, org accounts only)";
    case "openai":
      return "sk-admin-… (Admin key, not a project key)";
    case "xai":
      return "xAI management key";
    case "openrouter":
      return "Provisioning key, or an inference key";
    default:
      return "API key";
  }
}

// ---------------------------------------------------------------------------

function Switch({
  label,
  checked,
  disabled = false,
  onChange,
}: {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (v: boolean) => void | Promise<void>;
}) {
  return (
    <button
      type="button"
      role="switch"
      className="switch"
      aria-label={label}
      aria-checked={checked}
      disabled={disabled}
      onClick={() => void onChange(!checked)}
    />
  );
}

/**
 * Rebind the global shortcut by pressing it.
 *
 * Typing an accelerator string by hand means knowing that Tauri spells it
 * `CmdOrControl+Alt+U`, and getting it wrong costs you the shortcut you had.
 * Capturing the real keystroke is the only version of this that a user can
 * succeed at without reading documentation.
 */
function HotkeyField({
  accelerator,
  mac,
  onChange,
}: {
  accelerator: string;
  mac: boolean;
  onChange: (accelerator: string) => Promise<void>;
}) {
  const [capturing, setCapturing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!capturing) return;

    const onKey = (e: KeyboardEvent) => {
      // Capture phase, so the window's own Esc-to-close never sees these.
      e.preventDefault();
      e.stopPropagation();

      if (e.key === "Escape") {
        setCapturing(false);
        return;
      }
      const next = acceleratorFrom(e, mac);
      // Modifiers alone are not a shortcut yet — keep waiting for the key.
      if (!next) return;

      setCapturing(false);
      setError(null);
      void onChange(next).catch((err) => setError(String(err)));
    };

    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [capturing, mac, onChange]);

  return (
    <div style={{ display: "grid", gap: 4, justifyItems: "end" }}>
      <button
        type="button"
        className="btn"
        aria-label="Change the show/hide shortcut"
        data-capturing={capturing || undefined}
        onClick={() => {
          setError(null);
          setCapturing(true);
        }}
      >
        <code>{capturing ? "Press keys…" : hotkeyLabel(accelerator, mac)}</code>
      </button>
      {error && (
        <span className="field-hint" data-tone="danger">
          {error}
        </span>
      )}
    </div>
  );
}

/**
 * A `KeyboardEvent` as Tauri spells accelerators, or null when the press is not
 * a usable shortcut yet.
 *
 * Reads `code` rather than `key` so the binding follows the physical key: with
 * `key`, Alt on macOS turns U into `¨` and the accelerator would be recorded as
 * a dead key. Letters, digits and function keys only — no guessing at names for
 * keys this has not been checked against.
 */
function acceleratorFrom(e: KeyboardEvent, mac: boolean): string | null {
  const parts: string[] = [];
  if (mac ? e.metaKey : e.ctrlKey) parts.push("CmdOrControl");
  if (mac && e.ctrlKey) parts.push("Control");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  // A shortcut with no modifier would swallow that key everywhere on the system.
  if (parts.length === 0) return null;

  let key: string | null = null;
  if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3);
  else if (/^Digit[0-9]$/.test(e.code)) key = e.code.slice(5);
  else if (/^F([1-9]|1[0-9]|2[0-4])$/.test(e.code)) key = e.code;
  if (!key) return null;

  return [...parts, key].join("+");
}

function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: [T, string][];
  onChange: (v: T) => void | Promise<void>;
}) {
  return (
    <div className="segmented" role="group">
      {options.map(([v, label]) => (
        <button
          key={v}
          type="button"
          aria-pressed={v === value}
          onClick={() => void onChange(v)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
