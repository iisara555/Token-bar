import { useEffect } from "react";
import { api } from "../api";
import { enabledProviders, grandTotal, guardLoad, useUsage } from "../state/useUsage";
import { money, relativeTime, statusLabel, statusTone, tokens, totalTokens } from "../format";
import { ACCENT, type UsageSnapshot } from "../types";
import { ProviderLogo } from "../components/ProviderLogo";
import { StatusBar } from "../components/StatusBar";

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
  const load = guardLoad(view, snapshots);

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
          {/* Same rail as the bar, same reading. The popover is where someone
              looks *after* the rail caught their eye, so it has to be the thing
              they recognise at the top of it. */}
          <StatusBar load={load} warnAt={view?.warnAt ?? 0.8} inline />
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
                <span
                  className="chip-mark"
                  data-status={s?.status ?? "not_configured"}
                  style={{ ["--accent" as string]: ACCENT[p.id] }}
                >
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
                <span className="badge" data-tone={statusTone(s?.status ?? "not_configured")}>
                  {statusLabel(s?.status ?? "not_configured")}
                </span>
                <span className="popover-row-value num">{summaryValue(s)}</span>
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

function summaryValue(snapshot: UsageSnapshot | undefined): string {
  const fiveHour = snapshot?.limits?.fiveHour;
  const week = snapshot?.limits?.week;
  if (fiveHour || week) {
    const five = fiveHour ? `${Math.round(fiveHour.remainingPercent)}%` : "—";
    const seven = week ? `${Math.round(week.remainingPercent)}%` : "—";
    return `5H ${five} · W ${seven}`;
  }
  return money(snapshot?.costCents ?? snapshot?.balanceCents ?? null);
}
