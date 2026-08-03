import { useEffect } from "react";
import { api } from "../api";
import { enabledProviders, grandTotal, useUsage } from "../state/useUsage";
import { money, relativeTime, statusLabel, statusTone, tokens, totalTokens } from "../format";
import { ACCENT, type UsageSnapshot } from "../types";
import { ProviderLogo } from "../components/ProviderLogo";

export function Popover() {
  const { view, snapshots, ready, init } = useUsage();

  useEffect(() => {
    void init();
  }, [init]);

  // Esc closes the panel, matching every other transient popover on Windows.
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
        <div>
          <div className="section-title">Last {view?.windowDays ?? 30} days</div>
          <div className="num" style={{ fontSize: 28, fontWeight: 700, letterSpacing: "-0.02em" }}>
            {money(total.cents)}
          </div>
          {total.reporting !== total.total && (
            <div className="faint" style={{ fontSize: 11 }}>
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
                <span
                  className="chip-mark"
                  data-status={s?.status ?? "not_configured"}
                  style={{ ["--accent" as string]: ACCENT[p.id] }}
                >
                  <ProviderLogo provider={p.id} />
                </span>
                <span style={{ textAlign: "left", minWidth: 0 }}>
                  <div style={{ fontSize: 12.5, fontWeight: 600 }}>{p.name}</div>
                  <div className="faint" style={{ fontSize: 10.5 }}>
                    {s ? relativeTime(s.fetchedAt) : "no data"}
                    {tokenTotal > 0 && ` · ${tokens(tokenTotal)} tok`}
                  </div>
                </span>
                <span className="badge" data-tone={statusTone(s?.status ?? "not_configured")}>
                  {statusLabel(s?.status ?? "not_configured")}
                </span>
                <span className="num" style={{ fontSize: 13, fontWeight: 650 }}>
                  {summaryValue(s)}
                </span>
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
