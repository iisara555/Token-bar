import { useEffect } from "react";
import { api } from "../api";
import { enabledProviders, grandTotal, useUsage } from "../state/useUsage";
import { money, relativeTime, statusLabel, tokens, totalTokens } from "../format";
import { readingsFor } from "../readings";
import type { ProviderView, UsageSnapshot } from "../types";
import { ProviderLogo } from "../components/ProviderLogo";
import { useWindowFit } from "../state/useWindowFit";

export function Popover() {
  const { view, snapshots, ready, init } = useUsage();

  // Matches the `popover` window in tauri.conf.json.
  useWindowFit("popover", { width: 380, height: 480 });

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

  return (
    <div className="popover-root">
      <div className="glass popover">
        <div className="popover-head">
          <div className="section-title">Last {view?.windowDays ?? 30} days</div>
          <div className="popover-total num">{money(total.cents)}</div>
          {total.reporting !== total.total && (
            <div className="faint" style={{ fontSize: "var(--text-xs)" }}>
              {total.reporting} of {total.total} providers reporting a cost
            </div>
          )}
        </div>

        <div className="popover-list">
          {!ready && <div className="empty">Loading…</div>}

          {ready && providers.length === 0 && (
            <div className="empty">
              <strong>Nothing enabled yet</strong>
              <span>Add a provider key in Settings to see spend here.</span>
            </div>
          )}

          {providers.map((p) => {
            const s = snapshots[p.id];
            const tokenTotal = s ? totalTokens(s.tokens) : 0;
            return (
              <button
                key={p.id}
                type="button"
                className="popover-row"
                onClick={() => void api.refresh(p.id)}
                title={`Refresh ${p.name}`}
              >
                <span className="chip-mark" data-status={s?.status ?? "not_configured"}>
                  <ProviderLogo provider={p.id} />
                </span>
                {/* Spans, not divs: a <button>'s content model is phrasing
                    content, and flow content inside one is invalid markup that
                    assistive tech is entitled to flatten. */}
                <span className="popover-row-name">
                  <span className="popover-row-title">{p.name}</span>
                  <span className="faint popover-row-meta">
                    {s ? relativeTime(s.fetchedAt) : "no data"}
                    {tokenTotal > 0 && ` · ${tokens(tokenTotal)} tok`}
                  </span>
                </span>
                <span className="popover-row-value num">{summaryValue(p, s)}</span>
              </button>
            );
          })}
        </div>

        <div className="popover-foot">
          <button type="button" className="btn" onClick={() => void api.refresh()}>
            Refresh all
          </button>
          <span style={{ display: "flex", gap: 8 }}>
            <button type="button" className="btn" onClick={() => void api.openSettings()}>
              Settings
            </button>
            <button
              type="button"
              className="btn"
              data-tone="danger"
              onClick={() => void api.quit()}
            >
              Quit
            </button>
          </span>
        </div>
      </div>
    </div>
  );
}

/**
 * The one-line version of what the bar plots.
 *
 * Built from the same readings the meters use, so a provider cannot summarise
 * itself differently here than it does two windows away.
 */
function summaryValue(provider: ProviderView, snapshot: UsageSnapshot | undefined): string {
  const readings = readingsFor(provider, snapshot);
  if (readings.length === 0) return statusLabel(snapshot?.status ?? "not_configured");
  return readings
    .map((r) =>
      // A money reading already carries its own currency mark; prefixing the
      // "$" tag as well reads as "$ $25.82". The tag earns its place only where
      // it names a window the value cannot.
      r.shortLabel === "$" ? r.value : `${r.shortLabel} ${r.value}`,
    )
    .join(" · ");
}
