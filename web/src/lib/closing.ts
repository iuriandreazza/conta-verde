const MONTHS = [
  "janeiro",
  "fevereiro",
  "março",
  "abril",
  "maio",
  "junho",
  "julho",
  "agosto",
  "setembro",
  "outubro",
  "novembro",
  "dezembro",
];

/** The last day of a month written `YYYY-MM`, which is what the period lock closes through (ADR 0069). */
export function lastDayOfMonth(month: string): string {
  const [year, number] = month.split("-").map(Number);
  // Day 0 of the next month is the last day of this one.
  const last = new Date(Date.UTC(year, number, 0)).getUTCDate();
  return `${month}-${String(last).padStart(2, "0")}`;
}

/** `YYYY-MM` moved by a number of months, across the turn of the year. */
function addMonths(month: string, count: number): string {
  const [year, number] = month.split("-").map(Number);
  const index = year * 12 + (number - 1) + count;
  return `${Math.floor(index / 12)}-${String((index % 12) + 1).padStart(2, "0")}`;
}

/**
 * The month a person most likely closes next, as `YYYY-MM`: the one after the lock (or the last one over when nothing
 * is closed), only once it is over. `today` is where the person is: the server reads no clock.
 */
export function suggestedMonthToClose(lockedThrough: string | null, today: string): string | undefined {
  const current = today.slice(0, 7);
  const next = lockedThrough === null ? addMonths(current, -1) : addMonths(lockedThrough.slice(0, 7), 1);
  // `YYYY-MM` strings compare as the months do.
  return next < current ? next : undefined;
}

/** `2026-10-31` as `outubro de 2026`. */
export function monthLabel(day: string): string {
  const [year, month] = day.split("-").map(Number);
  return `${MONTHS[month - 1]} de ${year}`;
}
