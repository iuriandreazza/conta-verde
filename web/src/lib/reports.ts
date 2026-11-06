const two = (value: number) => String(value).padStart(2, "0");

/** This month, from its first day to its last, as `YYYY-MM-DD`: the span the entries report opens with. */
export function monthSpan(today: string): { from: string; to: string } {
  const [year, month] = today.split("-").map(Number);
  // Day 0 of the next month is the last day of this one.
  const last = new Date(Date.UTC(year, month, 0)).getUTCDate();
  return { from: `${year}-${two(month)}-01`, to: `${year}-${two(month)}-${two(last)}` };
}

/** A name cut to fit beside a bar, with an ellipsis. */
export function fitted(name: string, max = 18): string {
  return name.length <= max ? name : `${name.slice(0, max - 1)}…`;
}
