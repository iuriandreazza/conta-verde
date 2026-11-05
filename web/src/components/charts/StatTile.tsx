import type { ReactNode } from "react";

/** A trend in the de-emphasis gray with the current point in the accent. Values are drawing numbers. */
export function Sparkline({ values }: { values: number[] }) {
  const width = 120;
  const height = 36;
  const pad = 5;
  const min = Math.min(...values);
  const span = Math.max(...values) - min || 1;
  const point = (value: number, index: number) => [
    pad + (index / (values.length - 1)) * (width - 2 * pad),
    pad + (1 - (value - min) / span) * (height - 2 * pad),
  ];
  const coordinates = values.map(point);
  const [lastX, lastY] = coordinates[coordinates.length - 1];
  return (
    <svg className="sparkline" width={width} height={height} aria-hidden>
      <polyline
        fill="none"
        stroke="var(--chart-muted)"
        strokeWidth={2}
        strokeLinejoin="round"
        strokeLinecap="round"
        points={coordinates.map(([cx, cy]) => `${cx},${cy}`).join(" ")}
      />
      <circle cx={lastX} cy={lastY} r={4} fill="var(--series-1)" stroke="var(--surface)" strokeWidth={2} />
    </svg>
  );
}

interface StatTileProps {
  label: string;
  value: string;
  note?: ReactNode;
  trend?: number[];
  /** The tile is about something that needs attention; the caller supplies the icon and label. */
  tone?: "critical";
}

export function StatTile({ label, value, note, trend, tone }: StatTileProps) {
  return (
    <div className={`card stat-tile${tone ? ` stat-tile-${tone}` : ""}`}>
      <p className="stat-label">{label}</p>
      <p className="stat-value">{value}</p>
      <div className="stat-foot">
        {note && <p className="stat-note">{note}</p>}
        {trend && <Sparkline values={trend} />}
      </div>
    </div>
  );
}
