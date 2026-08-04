/**
 * Daily spend, as a row of lit cells.
 *
 * The adapters have always built this series and the cache has always stored
 * it; until now nothing drew it, so every snapshot carried up to thirty days of
 * history across the IPC boundary to be thrown away. This is the surface that
 * data was fetched for.
 *
 * It is bars rather than a line. A line implies a continuous quantity sampled
 * at points, and spend is not that: each day is a closed bucket with a total,
 * and the gap between two days is not a slope anyone should read. Bars also let
 * a zero day stay legible as *zero* rather than as a dip in a curve.
 *
 * Deliberately unlabelled. There is no y-axis, no gridline and no per-bar
 * figure — the card already prints the period total above it, and the question
 * this answers is "was it steady, or was it one bad Tuesday", which is shape.
 * Anything more would be a chart where the design asked for a texture.
 */

import { money } from "../format";
import type { Bucket } from "../types";

interface SparklineProps {
  series: Bucket[];
  /** Same window the card's footer names, for the caption. */
  windowDays: number;
}

export function Sparkline({ series, windowDays }: SparklineProps) {
  // A single bucket has no shape to show: one bar at full height says only
  // that the day is the largest of the one day on record.
  const days = series.filter((b) => b.costCents !== null);
  if (days.length < 2) return null;

  const values = days.map((b) => b.costCents ?? 0);
  const peak = Math.max(...values);
  // Every bar would be full height against a zero peak, which reads as heavy
  // spend on a period where nothing was spent at all.
  if (peak <= 0) return null;

  const total = values.reduce((sum, v) => sum + v, 0);

  return (
    <div className="spark">
      <div className="spark-head">
        <span className="spark-label">Daily</span>
        <span className="spark-peak faint">Peak {money(peak)}</span>
      </div>

      <div
        className="spark-bars"
        role="img"
        aria-label={`Daily spend over the last ${days.length} days, ${money(total)} in total, highest day ${money(peak)}`}
      >
        {days.map((bucket, i) => {
          const value = bucket.costCents ?? 0;
          return (
            <span
              className="spark-bar"
              key={bucket.start || i}
              // A day with real spend never rounds away to nothing: a bar that
              // vanishes is indistinguishable from a day that never loaded,
              // and those are opposite facts.
              style={{ height: `${value > 0 ? Math.max((value / peak) * 100, 6) : 0}%` }}
              data-empty={value === 0 || undefined}
              title={`${dayLabel(bucket.start)} · ${money(value)}`}
            />
          );
        })}
      </div>

      <div className="spark-foot faint">
        <span>{days.length === windowDays ? `${windowDays} days` : `${days.length} days`}</span>
        <span className="num">{money(total)}</span>
      </div>
    </div>
  );
}

/** "Aug 3" — enough to find the day in the tooltip, no more. */
function dayLabel(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "—";
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}
