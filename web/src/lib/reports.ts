import { monthsSpan } from "./period";

/** This month, from its first day to its last, as `YYYY-MM-DD`: the span the entries report opens with. */
export function monthSpan(today: string): { from: string; to: string } {
  const [year, month] = today.split("-").map(Number);
  return monthsSpan(year, month, 1);
}

/** A name cut to fit beside a bar, with an ellipsis. */
export function fitted(name: string, max = 18): string {
  return name.length <= max ? name : `${name.slice(0, max - 1)}…`;
}
