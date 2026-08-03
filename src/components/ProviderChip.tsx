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

  // A provider with nothing to plot is shown as its mark and status dot alone.
  //
  // The bar's width is a running cost, and it should be spent on readings. Three
  // un-set-up providers spelling out "Sign in", "Key rejected" and "No key" took
  // more of the row than the one provider actually reporting numbers — the bar
  // was widest exactly when it had least to say. The dot already carries the
  // state (and carries it by position and colour, not colour alone), the title
  // attribute still spells it out on hover, and clicking still opens the card
  // where the full explanation and the fix live.
  const bare = readings.length === 0;

  return (
    <button
      type="button"
      className="chip"
      data-bare={bare || undefined}
      aria-expanded={expanded}
      onClick={onToggle}
      title={`${provider.name} — ${needsSignIn ? "Sign in" : statusLabel(status)}`}
    >
      <span className="chip-mark" data-status={status}>
        {/* Smaller than the 18px default the popover and settings rows use: on
            the bar the mark is only there to say *which* provider a reading
            belongs to, and it sits next to type at 10-13px. */}
        <ProviderLogo provider={provider.id} size={16} />
      </span>

      <span className="chip-meters">
        {bare ? (
          // Nothing to plot — and deliberately no placeholder bar, which would
          // read as "zero left" rather than "no data". The label is kept for
          // screen readers, where there is no dot to see and no width to save.
          <span className="sr-only">{needsSignIn ? "Sign in" : statusLabel(status)}</span>
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
