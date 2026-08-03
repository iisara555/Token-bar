import { useEffect } from "react";
import { api } from "../api";
import { enabledProviders, grandTotal, useUsage } from "../state/useUsage";
import { money, relativeTime, statusLabel, statusTone } from "../format";
import { readingsFor, tightestHeadroom } from "../readings";
import type { ProviderView, UsageSnapshot } from "../types";
import { ProviderLogo } from "../components/ProviderLogo";
import { CloseIcon, RefreshIcon, SettingsIcon } from "../components/Icons";

/**
 * The status panel, opened from the tray.
 *
 * The bar is glanceable; this is the thing you actually stop and read. It
 * repeats no meter from the bar — instead every provider is one row of plain
 * label/value pairs, which is denser than a stack of tracks and is what makes a
 * dozen readings scannable in a column this narrow.
 */
export function Popover() {
  const { view, snapshots, ready, init } = useUsage();

  useEffect(() => {
    void init();
  }, [init]);

  // Esc closes the panel, matching every other transient popover on both
  // platforms — a menu bar extra's panel on macOS dismisses the same way.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") void api.hideWindow("popover");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const providers = enabledProviders(view);
  const total = grandTotal(view, snapshots);
  const headroom = tightestHeadroom(
    providers.map((p) => ({ provider: p, snapshot: snapshots[p.id] })),
  );

  return (
    <div className="panel-root">
      <div className="glass panel">
        <div className="panel-actions">
          <button
            type="button"
            className="orb"
            onClick={() => void api.refresh()}
            title="Refresh all providers"
          >
            <RefreshIcon />
            <span className="sr-only">Refresh all providers</span>
          </button>
          <button
            type="button"
            className="orb"
            onClick={() => void api.openSettings()}
            title="Settings"
          >
            <SettingsIcon />
            <span className="sr-only">Settings</span>
          </button>
          <button
            type="button"
            className="orb orb--close"
            onClick={() => void api.hideWindow("popover")}
            title="Close"
          >
            <CloseIcon />
            <span className="sr-only">Close</span>
          </button>
        </div>

        <div className="panel-head">
          <div className="panel-total num">{money(total.cents)}</div>

          {headroom && (
            <div
              className="meter-track meter-track--lg"
              data-ramp={headroom.ramp}
              role="meter"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={Math.round(headroom.percent)}
              aria-valuetext={headroom.label}
              aria-label="Headroom at the tightest limit being tracked"
              title={headroom.label}
            >
              <span
                className="meter-fill"
                style={{ width: `${Math.max(headroom.percent, 2.5)}%` }}
              />
            </div>
          )}

          {/* The bar measures headroom; the figure above it is money. Two
              unrelated quantities stacked without a word between them invite
              being read as one, so the bar gets its own line and the money
              keeps the reporting count. */}
          {headroom && <div className="faint panel-note">{headroom.label}</div>}
          <div className="faint panel-note">
            {total.reporting === total.total
              ? `Last ${view?.windowDays ?? 30} days`
              : `${total.reporting} of ${total.total} providers reporting a cost`}
          </div>
        </div>

        <div className="panel-list">
          {!ready && <div className="empty">Loading…</div>}

          {ready && providers.length === 0 && (
            <div className="empty">
              <strong>Nothing enabled yet</strong>
              <span>Add a provider key in Settings to see usage here.</span>
            </div>
          )}

          {providers.map((p) => (
            <ProviderRow key={p.id} provider={p} snapshot={snapshots[p.id]} />
          ))}
        </div>

        <footer className="panel-foot">
          <span className="wordmark num">Quoken</span>
          <span className="credit">
            <span className="credit-label">Powered by</span>
            <span className="credit-name">Itsara Kaewruang</span>
          </span>
        </footer>
      </div>
    </div>
  );
}

function ProviderRow({
  provider,
  snapshot,
}: {
  provider: ProviderView;
  snapshot: UsageSnapshot | undefined;
}) {
  const status = snapshot?.status ?? "not_configured";
  const readings = readingsFor(provider, snapshot);
  const tone = statusTone(status);
  const needsSignIn = provider.usesOauth && status === "auth_error";

  return (
    <button
      type="button"
      className="panel-row"
      onClick={() => void api.refresh(provider.id)}
      title={`Refresh ${provider.name}`}
    >
      <span className="panel-row-logo">
        <ProviderLogo provider={provider.id} size={30} />
      </span>

      <span className="panel-row-id">
        <span className="panel-row-name">{provider.name}</span>
        {/* When something is wrong, that replaces the timestamp rather than
            sitting beside it — a row cannot be both fine and broken, and the
            fresher of two truths is not the useful one here. */}
        <span className="panel-row-sub" data-tone={tone === "ok" ? undefined : tone}>
          {tone === "ok"
            ? snapshot
              ? relativeTime(snapshot.fetchedAt)
              : "no data"
            : needsSignIn
              ? "sign in"
              : statusLabel(status).toLowerCase()}
        </span>
      </span>

      {/* Paired readings ("5 Hour 76%" over "Weekly 59%") sit label-beside-value
          so the two numerals form a column. A lone reading has a much longer
          label — "Total available" — and putting that beside its value eats the
          width the provider's name needs, so it stacks instead. */}
      <span className="panel-row-readings" data-stacked={readings.length === 1}>
        {readings.map((r) => (
          <span className="panel-reading" key={r.key}>
            <span className="panel-reading-label">{r.panelLabel}</span>
            <span className="panel-reading-value num">{r.value}</span>
          </span>
        ))}
      </span>
    </button>
  );
}
