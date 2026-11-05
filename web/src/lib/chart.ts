import { useEffect, useState, type RefObject } from "react";
import { formatDecimal } from "./decimal";

/**
 * The only place an amount becomes a `Number`, and only to place a mark on screen: a sub-cent error
 * here moves a pixel, never a figure. Text a person reads is always the original decimal string
 * through `formatMoney` (ADR 0003, ADR 0020).
 */
export function toDrawing(value: string): number {
  return Number(value);
}

/** The axis top and step: round numbers, at most `maxTicks` intervals, never below `max`. */
export function niceScale(max: number, maxTicks = 4): { top: number; step: number } {
  if (max <= 0) return { top: 1, step: 1 };
  const rough = max / maxTicks;
  const magnitude = 10 ** Math.floor(Math.log10(rough));
  const step = [1, 2, 2.5, 5, 10].map((factor) => factor * magnitude).find((candidate) => candidate >= rough)!;
  return { top: Math.ceil(max / step) * step, step };
}

export function ticksOf({ top, step }: { top: number; step: number }): number[] {
  return Array.from({ length: Math.round(top / step) + 1 }, (_, index) => index * step);
}

/** An axis tick such as `R$ 12,5 mil`; ticks are round numbers picked by `niceScale`, not data. */
export function tickLabel(value: number): string {
  return value >= 1000 ? `R$ ${formatDecimal(String(value / 1000))} mil` : `R$ ${value}`;
}

/** Follows the width of an element, so a chart redraws instead of shrinking its text. */
export function useElementWidth(ref: RefObject<HTMLElement | null>, fallback = 640): number {
  const [width, setWidth] = useState(fallback);
  useEffect(() => {
    const element = ref.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(([entry]) => setWidth(Math.round(entry.contentRect.width)));
    observer.observe(element);
    return () => observer.disconnect();
  }, [ref]);
  return width;
}
