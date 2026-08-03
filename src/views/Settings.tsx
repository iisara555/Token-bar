import { useEffect, useState } from "react";
import { api } from "../api";
import { useUsage } from "../state/useUsage";
import {
  ACCENT,
  type AuthMode,
  type GlassPref,
  type ProviderId,
  type ProviderView,
  type ThemeMode,
} from "../types";
import { CloseIcon, KeyIcon } from "../components/Icons";
import { ProviderLogo } from "../components/ProviderLogo";

export function Settings() {
  const { view, ready, init, reload } = useUsage();
  const [autostart, setAutostart] = useState(false);
  const [autostartReady, setAutostartReady] = useState(false);

  useEffect(() => {
    void init();
  }, [init]);

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
                  <div className="field-label">Theme</div>
                  <div className="field-hint">
                    System follows Windows light/dark automatically.
                  </div>
                </div>
                <Segmented<ThemeMode>
                  value={view.theme}
                  options={[
                    ["system", "System"],
                    ["light", "Light"],
                    ["dark", "Dark"],
                  ]}
                  onChange={async (v) => {
                    await api.setTheme(v);
                    await reload();
                  }}
                />
              </div>

              <div className="field">
                <div>
                  <div className="field-label">Glass</div>
                  <div className="field-hint">
                    Currently rendering in <strong>{view.glassMode}</strong> mode.
                    {view.glassMode === "solid" &&
                      " Windows transparency effects are off, or you turned glass off here."}
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
                    <code>{view.hotkey}</code> hotkey to hide it entirely.
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
                    Keep provider marks and the total while hiding secondary detail.
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
                  <div className="field-label">Start with Windows</div>
                  <div className="field-hint">
                    Launch Token Bar automatically after you sign in.
                  </div>
                </div>
                <Switch
                  label="Start with Windows"
                  checked={autostart}
                  disabled={!autostartReady}
                  onChange={async (v) => {
                    await api.setAutostart(v);
                    setAutostart(v);
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
              <p style={{ fontSize: 12, lineHeight: 1.55, margin: 0, color: "var(--text-dim)" }}>
                OAuth reads the existing <strong>Claude Code, Codex or Kimi Code login</strong>{" "}
                directly each time. Expired Claude Code access tokens are refreshed through
                Anthropic and the rotated pair is saved back to that official credentials file.
                The Link buttons open the provider page in Chrome; Token Bar never reads or copies
                Chrome cookies. API keys are stored in the <strong>Windows Credential Manager</strong> under{" "}
                <code>com.tokenbar.app</code>, never in a config file and never in this
                window — once saved, only the Rust process can read them, and it sends
                them nowhere but the provider&rsquo;s own API. Removing a key here deletes
                it from the credential store.
              </p>
            </div>
          </section>

          <div className="faint" style={{ fontSize: 11, textAlign: "center" }}>
            Token Bar {view.version}
          </div>
        </div>
      </div>
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
        <span className="chip-mark" style={{ ["--accent" as string]: ACCENT[provider.id] }}>
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
          {provider.oauthStatus !== null && (
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

          {!provider.caps.cost && (
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
