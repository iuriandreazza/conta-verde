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

/** The last day of a month written `YYYY-MM`, which is what the period lock closes through (ADR 0068). */
export function lastDayOfMonth(month: string): string {
  const [year, number] = month.split("-").map(Number);
  // Day 0 of the next month is the last day of this one.
  const last = new Date(Date.UTC(year, number, 0)).getUTCDate();
  return `${month}-${String(last).padStart(2, "0")}`;
}

/** `2026-10-31` as `outubro de 2026`. */
export function monthLabel(day: string): string {
  const [year, month] = day.split("-").map(Number);
  return `${MONTHS[month - 1]} de ${year}`;
}
