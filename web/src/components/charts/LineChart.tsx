import { useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { niceRange, rangeTicks, tickLabel, toDrawing, useElementWidth } from "../../lib/chart";
import { formatMoney } from "../../lib/decimal";
import type { LegendItem } from "./Legend";

export interface FlowPoint {
  label: string;
  /** One decimal string per series, in the order of `series`. */
  values: string[];
}

const HEIGHT = 260;
const MARGIN = { top: 12, bottom: 28, left: 72 };
const END_LABEL_ROOM = 76;

/**
 * Lines on one axis that may go below zero, with a crosshair that snaps to the nearest point. `unit`
 * names what a point is, for the keyboard hint ("mês", "dia").
 */
export function LineChart({
  points,
  series,
  label,
  unit = "mês",
}: {
  points: FlowPoint[];
  series: LegendItem[];
  label: string;
  unit?: string;
}) {
  const bodyRef = useRef<HTMLDivElement>(null);
  const width = useElementWidth(bodyRef);
  const [active, setActive] = useState<number | null>(null);

  const right = width >= 560 ? END_LABEL_ROOM : 16;
  const plotWidth = Math.max(width - MARGIN.left - right, 1);
  const plotHeight = HEIGHT - MARGIN.top - MARGIN.bottom;
  const numbers = points.map((point) => point.values.map(toDrawing));
  const scale = niceRange(Math.min(...numbers.flat()), Math.max(...numbers.flat()));
  const x = (index: number) => MARGIN.left + (points.length > 1 ? (index / (points.length - 1)) * plotWidth : plotWidth / 2);
  const y = (value: number) => MARGIN.top + plotHeight * (1 - (value - scale.bottom) / (scale.top - scale.bottom));
  const labelEvery = plotWidth / points.length < 44 ? 2 : 1;

  function nearest(event: PointerEvent<SVGRectElement>) {
    const box = event.currentTarget.getBoundingClientRect();
    const ratio = (event.clientX - box.left) / box.width;
    setActive(Math.min(points.length - 1, Math.max(0, Math.round(ratio * (points.length - 1)))));
  }

  function move(event: KeyboardEvent<SVGSVGElement>) {
    const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
    if (step === 0) return;
    event.preventDefault();
    setActive((current) => Math.min(points.length - 1, Math.max(0, (current ?? points.length - 1) + step)));
  }

  const lastY = series.map((_, index) => y(numbers[numbers.length - 1][index]));
  const labelsFit = lastY.every((value, index) => lastY.every((other, otherIndex) => index === otherIndex || Math.abs(value - other) >= 14));

  return (
    <div className="chart-body" ref={bodyRef}>
      <svg
        width={width}
        height={HEIGHT}
        role="group"
        aria-label={`${label}. Use as setas para ver cada ${unit}.`}
        tabIndex={0}
        onKeyDown={move}
        onFocus={() => setActive((current) => current ?? points.length - 1)}
        onBlur={() => setActive(null)}
      >
        {rangeTicks(scale).map((tick) => (
          <g key={tick}>
            <line className={tick === 0 ? "chart-axis" : "chart-grid"} x1={MARGIN.left} x2={MARGIN.left + plotWidth} y1={y(tick)} y2={y(tick)} />
            <text className="chart-tick" x={MARGIN.left - 8} y={y(tick) + 4} textAnchor="end">
              {tickLabel(tick)}
            </text>
          </g>
        ))}
        {points.map((point, index) =>
          index % labelEvery === 0 ? (
            <text key={point.label} className="chart-tick" x={x(index)} y={HEIGHT - 8} textAnchor="middle">
              {point.label}
            </text>
          ) : null,
        )}
        {active !== null && <line className="chart-crosshair" x1={x(active)} x2={x(active)} y1={MARGIN.top} y2={MARGIN.top + plotHeight} />}
        {series.map((item, seriesIndex) => (
          <g key={item.name}>
            <polyline
              fill="none"
              stroke={item.color}
              strokeWidth={2}
              strokeLinejoin="round"
              strokeLinecap="round"
              points={numbers.map((row, index) => `${x(index)},${y(row[seriesIndex])}`).join(" ")}
            />
            <circle
              cx={x(active ?? points.length - 1)}
              cy={y(numbers[active ?? points.length - 1][seriesIndex])}
              r={4}
              fill={item.color}
              stroke="var(--surface)"
              strokeWidth={2}
            />
            {labelsFit && right === END_LABEL_ROOM && (
              <text className="chart-end-label" x={x(points.length - 1) + 10} y={lastY[seriesIndex] + 4}>
                {item.name}
              </text>
            )}
          </g>
        ))}
        <rect
          x={MARGIN.left}
          y={MARGIN.top}
          width={plotWidth}
          height={plotHeight}
          fill="transparent"
          onPointerMove={nearest}
          onPointerLeave={() => setActive(null)}
        />
      </svg>
      {active !== null && (
        <div
          className="chart-tooltip"
          style={{
            left: x(active),
            top: MARGIN.top,
            transform: x(active) > width * 0.55 ? "translateX(calc(-100% - 12px))" : "translateX(12px)",
          }}
        >
          <p className="chart-tooltip-title">{points[active].label}</p>
          {series.map((item, index) => (
            <p key={item.name} className="chart-tooltip-row">
              <span className="legend-key legend-key-line" style={{ background: item.color }} aria-hidden />
              <strong>{formatMoney(points[active].values[index])}</strong>
              <span>{item.name}</span>
            </p>
          ))}
        </div>
      )}
    </div>
  );
}
