import type { GuardLoad } from "../state/useUsage";

interface StatusBarProps {
  /** The worst pressure across everything enabled, or null when nothing has a
      ceiling to measure against. */
  load: GuardLoad | null;
  /** Fraction of a budget at which the app starts warning. Marked on the rail. */
  warnAt: number;
  /** Drop the pill inset. Set where the rail is a section rule rather than the
      foot of the floating bar. */
  inline?: boolean;
}

/**
 * The guardian rail.
 *
 * One hairline across the full width of the bar that fills white → orange as
 * the tightest ceiling anywhere fills up. It is the only element in the app
 * meant to be read without being looked at: peripheral vision resolves position
 * and motion long before it resolves digits, so the rail carries *length* as
 * its signal and lets colour ride along as reinforcement.
 *
 * What it deliberately does not do:
 *
 *   - No number on it. The digits are already two millimetres above it.
 *   - No animation at rest. A pulsing element in permanent view is a tax paid
 *     every hour for information that changes twice a day.
 *   - No hue change to signal state. Length already carries the reading, so the
 *     ramp stays one continuous sweep and remains legible to anyone who cannot
 *     separate orange from green.
 */
export function StatusBar({ load, warnAt, inline = false }: StatusBarProps) {
  const ratio = load?.ratio ?? 0;

  // Past the ceiling the rail is full and there is nothing further to show by
  // length, so overflow is carried by state instead — see `data-state="over"`.
  const fill = Math.min(1, ratio);
  const percent = Math.round(ratio * 100);

  const state = load === null ? "dormant" : ratio >= 1 ? "over" : ratio >= warnAt ? "warn" : "normal";

  // A rail that vanishes at low values reads as broken rather than calm, and
  // the leading edge is what the eye actually tracks. Keep a sliver visible.
  const width = load === null ? 0 : Math.max(fill, 0.012) * 100;

  const label =
    load === null
      ? "No budget or rate limit is being tracked yet"
      : `${load.provider} ${load.reason}: ${percent}% used`;

  return (
    <div
      className={inline ? "guard guard--inline" : "guard"}
      data-state={state}
      role="meter"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.min(100, percent)}
      aria-valuetext={label}
      aria-label="Usage against the tightest limit being tracked"
      title={label}
    >
      <div className="guard-track">
        {/* The ramp is painted across the *track*, then revealed to the fill
            width. That is what makes the leading edge's colour mean something:
            pale at 20%, deep orange at 100%. A gradient rescaled to the fill
            would end in the same orange at every value. */}
        <div className="guard-fill" style={{ width: `${width}%` }}>
          <div className="guard-ramp" />
        </div>

        {/* Where "getting close" begins, so the fill is read against something
            rather than guessed at. Hidden once the fill has passed it — a
            threshold behind you is no longer a threshold. */}
        {load !== null && warnAt > 0 && warnAt < 1 && (
          <div
            className="guard-mark"
            style={{ left: `${warnAt * 100}%` }}
            aria-hidden
          />
        )}
      </div>
    </div>
  );
}
