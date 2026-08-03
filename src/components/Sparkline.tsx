import { useId } from "react";

interface SparklineProps {
  values: number[];
  color: string;
  width?: number;
  height?: number;
  /** Screen-reader description; the shape alone conveys nothing without it. */
  label: string;
}

/**
 * A bare trend line. No axes, no grid — at 44×16 any additional ink is noise.
 *
 * The baseline is always zero rather than the series minimum: on a spend chart
 * a min-anchored baseline turns a flat $4.90–$5.10 week into a dramatic
 * mountain range.
 */
export function Sparkline({
  values,
  color,
  width = 44,
  height = 16,
  label,
}: SparklineProps) {
  const gradientId = useId();

  if (values.length === 0) {
    return <svg width={width} height={height} className="chip-spark" aria-hidden />;
  }

  const max = Math.max(...values, 0);
  const pad = 1.5;
  const usableH = height - pad * 2;

  // A single reading has no trend to draw; show it as a flat line at mid-height
  // rather than an empty box, so the chip does not look broken.
  const points =
    values.length === 1
      ? [
          [0, height / 2],
          [width, height / 2],
        ]
      : values.map((v, i) => {
          const x = (i / (values.length - 1)) * width;
          const y = max === 0 ? height - pad : height - pad - (v / max) * usableH;
          return [x, y];
        });

  const line = points.map(([x, y]) => `${x.toFixed(1)},${y.toFixed(1)}`).join(" ");
  const area = `${line} ${width},${height} 0,${height}`;

  return (
    <svg
      width={width}
      height={height}
      className="chip-spark"
      role="img"
      aria-label={label}
      style={{ overflow: "visible" }}
    >
      <defs>
        <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={color} stopOpacity="0.34" />
          <stop offset="100%" stopColor={color} stopOpacity="0" />
        </linearGradient>
      </defs>
      <polygon points={area} fill={`url(#${gradientId})`} />
      <polyline
        points={line}
        fill="none"
        stroke={color}
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
