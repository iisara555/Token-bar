/**
 * The meter.
 *
 * Every reading in the app is one of these: a label, a track that fills with a
 * gradient, and a numeral. Two ramps and only two — warm for anything burning
 * down against a short clock (the 5-hour window, spend, a credit balance), cool
 * for the long horizon (the weekly window). Which ramp a meter wears says what
 * *kind* of limit it is, not how full it is, so the two never swap: a glance
 * can separate them before reading either.
 *
 * The gradient is scaled to the fill rather than painted across the track, so a
 * meter at 40% still ends in its ramp's full-strength colour. That makes a
 * short meter look like a short meter, not like a faded one.
 *
 * At card size the track gets a printed quarter graduation beneath it. At chip
 * size it does not: across 76px the ticks would be texture rather than a scale,
 * and the reading sits right beside the bar there anyway.
 */

export type Ramp = "warm" | "cool";

interface MeterProps {
  /** Short all-caps tag: "5H", "W", "$". */
  label: string;
  /** 0–100, or null when the provider did not report one. */
  percent: number | null;
  /** The reading itself — "96%", "$15.23". Rendered in the dot-matrix face. */
  value: string;
  /** Small line under the track: "Reset 3h". */
  caption?: string;
  ramp: Ramp;
  /** Full sentence for the tooltip and for screen readers. */
  description: string;
}

export function Meter({ label, percent, value, caption, ramp, description }: MeterProps) {
  return (
    <div className="meter" title={description}>
      <span className="meter-label" aria-hidden>
        {label}
      </span>
      <span className="meter-body">
        <Track percent={percent} ramp={ramp} description={description} />
        {caption && <span className="meter-caption">{caption}</span>}
      </span>
      <span className="meter-value num" aria-hidden>
        {value}
      </span>
    </div>
  );
}

interface MeterBlockProps {
  /** Full-length label: "5 hour limit". */
  label: string;
  percent: number | null;
  value: string;
  /** "Reset in 3h : Aug 3, 12:50 PM" */
  caption?: string;
  ramp: Ramp;
  description: string;
}

/** The same reading at card size: label and numeral above a full-width track. */
export function MeterBlock({
  label,
  percent,
  value,
  caption,
  ramp,
  description,
}: MeterBlockProps) {
  return (
    <div className="meter-block">
      <div className="meter-block-head">
        <span className="meter-block-label">{label}</span>
        <span className="meter-block-value num" aria-hidden>
          {value}
        </span>
      </div>
      <Track percent={percent} ramp={ramp} description={description} large />
      {/* Quarter graduation, printed under the track the way a dial's scale is
          printed on the face. Decoration to a screen reader — the numeral above
          and the meter's own aria-valuetext already say where the needle is. */}
      <span className="meter-scale" aria-hidden />
      {caption && <div className="meter-block-caption">{caption}</div>}
    </div>
  );
}

function Track({
  percent,
  ramp,
  description,
  large = false,
}: {
  percent: number | null;
  ramp: Ramp;
  description: string;
  large?: boolean;
}) {
  // An unreported value is not zero. The track stays empty and says so through
  // the label rather than drawing a full-looking bar at 0%.
  const width = percent === null ? 0 : clamp(percent);

  return (
    <span
      className={large ? "meter-track meter-track--lg" : "meter-track"}
      data-ramp={ramp}
      role="meter"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={percent === null ? 0 : Math.round(clamp(percent))}
      aria-valuetext={description}
    >
      {/* A reading that rounds to nothing still gets a lit cell: a track that
          empties completely is indistinguishable from one that never loaded. */}
      <span
        className="meter-fill"
        style={{ width: `${percent === null ? 0 : Math.max(width, 2.5)}%` }}
      />
    </span>
  );
}

function clamp(n: number): number {
  if (!Number.isFinite(n)) return 0;
  return Math.min(100, Math.max(0, n));
}
