import {
  ACCENT,
  type ProviderView,
  type QuotaWindow,
  type UsageSnapshot,
} from "../types";
import {
  budgetRatio,
  money,
  relativeTime,
  resetIn,
  resetAt,
  statusLabel,
  statusTone,
  tokens,
  totalTokens,
} from "../format";
import { useEffect, useState } from "react";
import { budgetCentsFor } from "../state/useUsage";
import { BudgetRing } from "./BudgetRing";
import { Sparkline } from "./Sparkline";
import { AlertIcon } from "./Icons";
import { ProviderLogo } from "./ProviderLogo";

interface DetailPanelProps {
  provider: ProviderView;
  snapshot: UsageSnapshot | undefined;
  warnAt: number;
  windowDays: number;
}

export function DetailPanel({
  provider,
  snapshot,
  warnAt,
  windowDays,
}: DetailPanelProps) {
  const [, refreshCountdown] = useState(0);
  useEffect(() => {
    const timer = window.setInterval(() => refreshCountdown((value) => value + 1), 60_000);
    return () => window.clearInterval(timer);
  }, []);

  const accent = ACCENT[provider.id];
  const status = snapshot?.status ?? "not_configured";
  const budget = budgetCentsFor(provider);
  const ratio = budgetRatio(snapshot?.costCents ?? null, budget);

  const series = (snapshot?.series ?? []).map((b) => b.costCents ?? 0);
  const t = snapshot?.tokens;
  const fiveHour = snapshot?.limits?.fiveHour;
  const week = snapshot?.limits?.week;
  const hasLimits = Boolean(fiveHour || week);

  return (
    <div className="glass panel">
      <div className="panel-head">
        <div className="panel-title">
          <span className="chip-mark" style={{ ["--accent" as string]: accent }}>
            <ProviderLogo provider={provider.id} />
          </span>
          {provider.name}
          <span className="badge" data-tone={statusTone(status)}>
            {provider.usesOauth && status === "auth_error" ? "Sign in" : statusLabel(status)}
          </span>
        </div>
        <div className="panel-meta">
          {snapshot ? `updated ${relativeTime(snapshot.fetchedAt)}` : "never updated"}
        </div>
      </div>

      {hasLimits ? (
        <div className="quota-grid">
          <QuotaCard label="5-hour limit" window={fiveHour} />
          <QuotaCard label="Weekly limit" window={week} />
        </div>
      ) : (
      <div className="panel-grid">
        <div>
          <div
            className="num"
            style={{ fontSize: 26, fontWeight: 700, letterSpacing: "-0.02em" }}
          >
            {money(snapshot?.costCents ?? null)}
          </div>
          <div className="faint" style={{ fontSize: 11 }}>
            last {windowDays} days
            {snapshot?.balanceCents != null &&
              ` · ${money(snapshot.balanceCents)} balance`}
          </div>

          {series.length > 1 && (
            <div style={{ marginTop: 10 }}>
              <Sparkline
                values={series}
                color={accent}
                width={220}
                height={40}
                label={`${provider.name} daily spend, last ${series.length} days`}
              />
            </div>
          )}
        </div>

        {ratio !== null && (
          <BudgetRing
            ratio={ratio}
            warnAt={warnAt}
            label={provider.name}
            sublabel={`of ${money(budget)}`}
          />
        )}
      </div>
      )}

      {t && totalTokens(t) > 0 && (
        <div className="stat-row">
          <div className="stat">
            <span className="dim">Input</span>
            <span className="num">{tokens(t.input)}</span>
          </div>
          <div className="stat">
            <span className="dim">Output</span>
            <span className="num">{tokens(t.output)}</span>
          </div>
          <div className="stat">
            <span className="dim">Cache read</span>
            <span className="num">{tokens(t.cacheRead)}</span>
          </div>
          <div className="stat">
            <span className="dim">Cache write</span>
            <span className="num">{tokens(t.cacheWrite)}</span>
          </div>
        </div>
      )}

      {snapshot?.message && (
        <div className="notice" data-tone={statusTone(status) === "danger" ? "danger" : undefined}>
          <AlertIcon />
          <span>{snapshot.message}</span>
        </div>
      )}

      {/* Say so when the number is inherently behind, rather than letting a
          day-old figure pass for a live one. */}
      {snapshot && !hasLimits && snapshot.sourceLatencySecs >= 3600 && status === "ok" && (
        <div className="notice">
          <AlertIcon />
          <span>
            {provider.name} reports in daily buckets — today&rsquo;s figure is
            partial until the day closes.
          </span>
        </div>
      )}
    </div>
  );
}

function QuotaCard({ label, window }: { label: string; window?: QuotaWindow | null }) {
  const remaining = window ? Math.round(window.remainingPercent) : null;
  const resetLabel = window?.resetsAt
    ? `${resetIn(window.resetsAt)} · ${resetAt(window.resetsAt)}`
    : "reset time unavailable";
  const tone =
    remaining === null
      ? "neutral"
      : remaining <= 20
        ? "danger"
        : remaining <= 40
          ? "warn"
          : "ok";

  return (
    <div className="quota-card" data-tone={tone}>
      <div className="quota-card-head">
        <span>{label}</span>
        <strong className="num">{remaining === null ? "—" : `${remaining}% left`}</strong>
      </div>
      <div className="quota-track" aria-hidden>
        <span style={{ width: `${remaining ?? 0}%` }} />
      </div>
      <div className="quota-reset">
        {resetLabel}
      </div>
    </div>
  );
}
