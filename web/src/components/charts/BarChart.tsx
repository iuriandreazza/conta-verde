import { useRef, useState } from "react";
import { niceScale, tickLabel, ticksOf, toDrawing, useElementWidth } from "../../lib/chart";
import { formatMoney } from "../../lib/decimal";
import { AlertGlyph } from "./AlertGlyph";
import type { LegendItem } from "./Legend";

export interface BarGroup {
  label: string;
  /** Drawn with an alert glyph, so the state never rests on color alone. */
  alert?: boolean;
  /** One decimal string per series, in the order of `series`. */
  values: string[];
}

const LABEL_ROOM = 132;
const VALUE_ROOM = 96;
const ROW = 64;
const BAR = 16;
const GAP = 2;
const AXIS = 28;

// 4px rounded at the data end, square at the baseline.
function barPath(x: number, y: number, length: number): string {
  const r = Math.min(4, length, BAR / 2);
  return `M${x},${y}h${length - r}a${r},${r} 0 0 1 ${r},${r}v${BAR - 2 * r}a${r},${r} 0 0 1 ${-r},${r}h${-(length - r)}z`;
}

/** Horizontal grouped bars: one row per category, one thin bar per series. */
export function BarChart({ groups, series, label }: { groups: BarGroup[]; series: LegendItem[]; label: string }) {
  const bodyRef = useRef<HTMLDivElement>(null);
  const width = useElementWidth(bodyRef);
  const [active, setActive] = useState<number | null>(null);

  const plotWidth = Math.max(width - LABEL_ROOM - VALUE_ROOM, 1);
  const scale = niceScale(Math.max(...groups.flatMap((group) => group.values.map(toDrawing))));
  const height = groups.length * ROW + AXIS;
  const x = (value: number) => LABEL_ROOM + (value / scale.top) * plotWidth;
  const stack = series.length * BAR + (series.length - 1) * GAP;

  return (
    <div className="chart-body" ref={bodyRef}>
      <svg width={width} height={height} role="group" aria-label={label}>
        {ticksOf(scale).map((tick) => (
          <g key={tick}>
            <line className={tick === 0 ? "chart-axis" : "chart-grid"} x1={x(tick)} x2={x(tick)} y1={0} y2={groups.length * ROW} />
            {(tick === 0 || plotWidth > 320) && (
              <text className="chart-tick" x={x(tick)} y={height - 8} textAnchor="middle">
                {tickLabel(tick)}
              </text>
            )}
          </g>
        ))}
        {groups.map((group, row) => {
          const top = row * ROW;
          const dimmed = active !== null && active !== row;
          return (
            <g
              key={group.label}
              tabIndex={0}
              role="group"
              aria-label={`${group.label}: ${series.map((item, index) => `${item.name} ${formatMoney(group.values[index])}`).join(", ")}`}
              opacity={dimmed ? 0.45 : 1}
              onPointerEnter={() => setActive(row)}
              onPointerLeave={() => setActive(null)}
              onFocus={() => setActive(row)}
              onBlur={() => setActive(null)}
            >
              <rect x={0} y={top} width={width} height={ROW} fill="transparent" />
              {group.alert && (
                <g transform={`translate(0 ${top + ROW / 2 - 7})`}>
                  <AlertGlyph />
                </g>
              )}
              <text className="chart-category" x={group.alert ? 22 : 0} y={top + ROW / 2 + 4}>
                {group.label}
              </text>
              {series.map((item, index) => {
                const length = Math.max((toDrawing(group.values[index]) / scale.top) * plotWidth, 0);
                const y = top + (ROW - stack) / 2 + index * (BAR + GAP);
                return (
                  <g key={item.name}>
                    {length > 0 && <path d={barPath(LABEL_ROOM, y, length)} fill={item.color} />}
                    <text className="chart-value" x={LABEL_ROOM + length + 8} y={y + BAR - 3}>
                      {formatMoney(group.values[index])}
                    </text>
                  </g>
                );
              })}
            </g>
          );
        })}
      </svg>
    </div>
  );
}
