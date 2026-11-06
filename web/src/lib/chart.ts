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

/**
 * The axis bottom, top and step for values that may go below zero: round numbers, at most `maxTicks`
 * intervals, the axis always reaching zero and never cutting a value off.
 */
export function niceRange(min: number, max: number, maxTicks = 4): { bottom: number; top: number; step: number } {
  const low = Math.min(min, 0);
  const high = Math.max(max, 0);
  if (high - low <= 0) return { bottom: 0, top: 1, step: 1 };
  const rough = (high - low) / maxTicks;
  const magnitude = 10 ** Math.floor(Math.log10(rough));
  const step = [1, 2, 2.5, 5, 10].map((factor) => factor * magnitude).find((candidate) => candidate >= rough)!;
  return { bottom: Math.floor(low / step) * step, top: Math.ceil(high / step) * step, step };
}

export function rangeTicks({ bottom, top, step }: { bottom: number; top: number; step: number }): number[] {
  return Array.from({ length: Math.round((top - bottom) / step) + 1 }, (_, index) => bottom + index * step);
}

/** An axis tick such as `R$ 12,5 mil`; ticks are round numbers picked by `niceScale`, not data. */
export function tickLabel(value: number): string {
  const sign = value < 0 ? "-" : "";
  const size = Math.abs(value);
  return size >= 1000 ? `R$ ${sign}${formatDecimal(String(size / 1000))} mil` : `R$ ${sign}${size}`;
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
