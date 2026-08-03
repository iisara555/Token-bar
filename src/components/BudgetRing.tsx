interface BudgetRingProps {
  /** 0..1 and beyond. Over-budget is shown, not clipped away. */
  ratio: number;
  warnAt: number;
  size?: number;
  label: string;
  sublabel?: string;
}

/**
 * Budget as a ring rather than a bar: at this size a ring gives the "how close
 * am I" answer in one glance and leaves its own middle free for the number.
 */
export function BudgetRing({
  ratio,
  warnAt,
  size = 64,
  label,
  sublabel,
}: BudgetRingProps) {
  const stroke = 6;
  const r = (size - stroke) / 2;
  const circumference = 2 * Math.PI * r;
  const clamped = Math.min(ratio, 1);
  const over = ratio > 1;

  const color = over
    ? "var(--danger)"
    : ratio >= warnAt
      ? "var(--warn)"
      : "var(--ok)";

  return (
    <div
      style={{ position: "relative", width: size, height: size, flex: "none" }}
      role="img"
      aria-label={`${label}: ${Math.round(ratio * 100)}% of budget used`}
    >
      <svg width={size} height={size} style={{ transform: "rotate(-90deg)" }}>
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          stroke="var(--track)"
          strokeWidth={stroke}
        />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          stroke={color}
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={circumference * (1 - clamped)}
          style={{ transition: "stroke-dashoffset var(--dur-slow) var(--ease)" }}
        />
      </svg>
      <div
        style={{
          position: "absolute",
          inset: 0,
          display: "grid",
          placeContent: "center",
          textAlign: "center",
          lineHeight: 1.1,
        }}
      >
        <div className="num" style={{ fontSize: 13, fontWeight: 700, color }}>
          {Math.round(ratio * 100)}%
        </div>
        {sublabel && (
          <div className="faint" style={{ fontSize: 9 }}>
            {sublabel}
          </div>
        )}
      </div>
    </div>
  );
}
