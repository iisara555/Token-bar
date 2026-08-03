import {
  ACCENT,
  type ProviderView,
  type QuotaWindow,
  type UsageSnapshot,
} from "../types";
import { useEffect, useState } from "react";
import { money, resetIn, resetInShort, statusLabel, tokens, totalTokens } from "../format";
import { Sparkline } from "./Sparkline";
import { ProviderLogo } from "./ProviderLogo";

interface ProviderChipProps {
  provider: ProviderView;
  snapshot: UsageSnapshot | undefined;
  expanded: boolean;
  onToggle: () => void;
}

export function ProviderChip({
  provider,
  snapshot,
  expanded,
  onToggle,
}: ProviderChipProps) {
  const [, refreshCountdown] = useState(0);
  useEffect(() => {
    const timer = window.setInterval(() => refreshCountdown((value) => value + 1), 60_000);
    return () => window.clearInterval(timer);
  }, []);

  const accent = ACCENT[provider.id];
  const status = snapshot?.status ?? "not_configured";
  const fiveHour = snapshot?.limits?.fiveHour;
  const week = snapshot?.limits?.week;
  const hasLimits = Boolean(fiveHour || week);

  // Cost is the headline where a provider reports it. Where it does not
  // (DeepSeek, until two balance readings exist) the remaining balance is the
  // honest thing to show instead of a fabricated zero.
  const primary =
    snapshot?.costCents !== null && snapshot?.costCents !== undefined
      ? money(snapshot.costCents)
      : snapshot?.balanceCents !== null && snapshot?.balanceCents !== undefined
        ? money(snapshot.balanceCents)
        : "—";

  const primaryIsBalance =
    (snapshot?.costCents === null || snapshot?.costCents === undefined) &&
    snapshot?.balanceCents !== null &&
    snapshot?.balanceCents !== undefined;

  const tokenTotal = snapshot ? totalTokens(snapshot.tokens) : 0;
  const sub =
    status === "ok" || status === "stale"
      ? primaryIsBalance
        ? "left"
        : tokenTotal > 0
          ? `${tokens(tokenTotal)} tok`
          : provider.name
      : statusLabel(status);

  const series = (snapshot?.series ?? []).map((b) =>
    b.costCents !== null ? b.costCents : totalTokens(b.tokens),
  );

  return (
    <button
      type="button"
      className="chip"
      aria-expanded={expanded}
      onClick={onToggle}
      title={`${provider.name} — ${provider.usesOauth && status === "auth_error" ? "Sign in" : statusLabel(status)}`}
    >
      <span
        className="chip-mark"
        data-status={status}
        style={{ ["--accent" as string]: accent }}
      >
        <ProviderLogo provider={provider.id} />
      </span>

      {hasLimits ? (
        <span className="quota-pair">
          <QuotaStat label="5H" window={fiveHour} />
          <QuotaStat label="WEEK" window={week} />
        </span>
      ) : (
        <span className="chip-body">
          <span className="chip-value num">{primary}</span>
          <span className="chip-sub">
            {provider.usesOauth && status === "auth_error" ? "Sign in" : sub}
          </span>
        </span>
      )}

      {!hasLimits && provider.caps.series && series.length > 1 && (
        <Sparkline
          values={series}
          color={accent}
          label={`${provider.name} daily trend`}
        />
      )}
    </button>
  );
}

function QuotaStat({ label, window }: { label: string; window?: QuotaWindow | null }) {
  const remaining = window ? Math.round(window.remainingPercent) : null;
  const tone = remaining === null ? "neutral" : remaining <= 20 ? "danger" : remaining <= 40 ? "warn" : "ok";
  return (
    <span
      className="quota-mini"
      data-tone={tone}
      title={`${label}: ${remaining === null ? "unknown" : `${remaining}% left`} · ${resetIn(window?.resetsAt)}`}
    >
      <span className="quota-mini-value num">{remaining === null ? "—" : `${remaining}%`}</span>
      <span className="quota-mini-label">{label} LEFT</span>
      <span className="quota-mini-reset">RESET {resetInShort(window?.resetsAt)}</span>
    </span>
  );
}
