import { useEffect, useState } from "react";
import { statusLabel } from "../format";
import { readingsFor } from "../readings";
import type { ProviderView, UsageSnapshot } from "../types";
import { Meter } from "./Meter";
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
  // Reset countdowns are rendered text, not live elements, so nothing redraws
  // them on its own. A minute is the resolution they are written at.
  const [, tick] = useState(0);
  useEffect(() => {
    const timer = window.setInterval(() => tick((n) => n + 1), 60_000);
    return () => window.clearInterval(timer);
  }, []);

  const status = snapshot?.status ?? "not_configured";
  const readings = readingsFor(provider, snapshot);
  const needsSignIn = provider.usesOauth && status === "auth_error";

  return (
    <button
      type="button"
      className="chip"
      aria-expanded={expanded}
      onClick={onToggle}
      title={`${provider.name} — ${needsSignIn ? "Sign in" : statusLabel(status)}`}
    >
      <span className="chip-mark" data-status={status}>
        <ProviderLogo provider={provider.id} />
      </span>

      <span className="chip-meters">
        {readings.length === 0 ? (
          // Nothing to plot. Say why rather than drawing an empty instrument,
          // which would read as "zero left" instead of "no data".
          <span className="chip-status">{needsSignIn ? "Sign in" : statusLabel(status)}</span>
        ) : (
          readings.map((r) => (
            <Meter
              key={r.key}
              label={r.shortLabel}
              percent={r.percent}
              value={r.value}
              caption={r.shortCaption}
              ramp={r.ramp}
              description={r.description}
            />
          ))
        )}
      </span>
    </button>
  );
}
