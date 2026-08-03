import { useEffect, useState } from "react";
import { money, relativeTime, statusLabel, statusTone, tokens, totalTokens } from "../format";
import { authKind, readingsFor } from "../readings";
import type { ProviderView, UsageSnapshot } from "../types";
import { AlertIcon } from "./Icons";
import { MeterBlock } from "./Meter";

interface DetailPanelProps {
  provider: ProviderView;
  snapshot: UsageSnapshot | undefined;
  windowDays: number;
}

/**
 * The card behind a provider in the bar.
 *
 * Same readings as the chip, at a size where the reset time can be a wall clock
 * rather than a countdown and the numeral can be the largest thing on screen.
 * Nothing new is introduced here that the bar did not already imply — opening
 * a card should feel like leaning closer, not like navigating somewhere.
 */
export function DetailPanel({ provider, snapshot, windowDays }: DetailPanelProps) {
  const [, tick] = useState(0);
  useEffect(() => {
    const timer = window.setInterval(() => tick((n) => n + 1), 60_000);
    return () => window.clearInterval(timer);
  }, []);

  const status = snapshot?.status ?? "not_configured";
  const readings = readingsFor(provider, snapshot);
  const kind = authKind(provider);
  const t = snapshot?.tokens;

  // The money reading that is not the headline.
  //
  // On a subscription that is the usage credits accrued alongside the rate
  // limits; on a pay-as-you-go account it is the spend that the balance was
  // drawn down from — the same figure the meter above uses as its denominator,
  // which is worth stating outright rather than leaving implied by a bar
  // length. Where spend *is* the headline there is nothing left to add.
  const cost = snapshot?.costCents;
  const secondary =
    cost === null || cost === undefined
      ? null
      : readings.some((r) => r.key === "fiveHour" || r.key === "week")
        ? { label: "Usage credits", value: money(cost) }
        : readings.some((r) => r.key === "balance")
          ? { label: "Total spend", value: money(cost) }
          : null;

  return (
    <div className="glass card-detail">
      <header className="card-head">
        <h2 className="card-title">{provider.name}</h2>
        <span className="card-auth" data-kind={kind}>
          {kind === "oauth" ? "Oauth" : "API"}
        </span>
      </header>

      <div className="card-rule" aria-hidden />

      {readings.length === 0 ? (
        <div className="card-empty">
          <span className="badge" data-tone={statusTone(status)}>
            {provider.usesOauth && status === "auth_error" ? "Sign in" : statusLabel(status)}
          </span>
          <span className="faint">
            {snapshot ? "Nothing to report yet." : "No data has arrived yet."}
          </span>
        </div>
      ) : (
        <div className="card-meters">
          {readings.map((r) => (
            <MeterBlock
              key={r.key}
              label={r.longLabel}
              percent={r.percent}
              value={r.value}
              caption={r.longCaption}
              ramp={r.ramp}
              description={r.description}
            />
          ))}
        </div>
      )}

      {secondary && (
        <div className="card-total">
          <span className="card-total-label">{secondary.label}</span>
          <span className="card-total-value num">{secondary.value}</span>
        </div>
      )}

      {t && totalTokens(t) > 0 && (
        <div className="card-stats">
          <Stat label="Input" value={tokens(t.input)} />
          <Stat label="Output" value={tokens(t.output)} />
          <Stat label="Cache read" value={tokens(t.cacheRead)} />
          <Stat label="Cache write" value={tokens(t.cacheWrite)} />
        </div>
      )}

      <footer className="card-foot">
        <span className="faint">
          {snapshot ? `Updated ${relativeTime(snapshot.fetchedAt)}` : "Never updated"}
        </span>
        <span className="faint">Last {windowDays} days</span>
      </footer>

      {snapshot?.message && (
        <div className="notice" data-tone={statusTone(status) === "danger" ? "danger" : undefined}>
          <AlertIcon />
          <span>{snapshot.message}</span>
        </div>
      )}

      {/* Say so when the number is inherently behind, rather than letting a
          day-old figure pass for a live one. */}
      {snapshot &&
        readings.every((r) => r.key !== "fiveHour" && r.key !== "week") &&
        snapshot.sourceLatencySecs >= 3600 &&
        status === "ok" && (
          <div className="notice">
            <AlertIcon />
            <span>
              {provider.name} reports in daily buckets — today&rsquo;s figure is partial
              until the day closes.
            </span>
          </div>
        )}
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat">
      <span className="dim">{label}</span>
      <span className="num">{value}</span>
    </div>
  );
}
