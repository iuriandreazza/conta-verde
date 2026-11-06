import type { CashFlowPeriod } from "../api";
import { centsToApi, sumCents } from "./entryInstallments";

const MONTHS = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];

const two = (value: number) => String(value).padStart(2, "0");

/** The span the page opens with: this month and the two after it, as `YYYY-MM-DD`. */
export function defaultSpan(today: string): { from: string; to: string } {
  const [year, month] = today.split("-").map(Number);
  // Day 0 of the month after next is the last day of the one before it.
  const last = new Date(Date.UTC(year, month + 2, 0));
  return {
    from: `${year}-${two(month)}-01`,
    to: `${last.getUTCFullYear()}-${two(last.getUTCMonth() + 1)}-${two(last.getUTCDate())}`,
  };
}

/** `2026-12-10` as `10/12/26` (a span of 400 days can repeat a day and month), or `2026-12-01` as `dez/2026` for months. */
export function periodLabel(period: CashFlowPeriod, granularity: "day" | "month"): string {
  const [year, month, day] = period.start.split("-").map(Number);
  return granularity === "day" ? `${two(day)}/${two(month)}/${two(year % 100)}` : `${MONTHS[month - 1]}/${year}`;
}

/** What the periods received and paid in all, to the cent. Neither is ever negative. */
export function totalsOf(periods: CashFlowPeriod[]): { received: string; paid: string } {
  const sum = (amounts: string[]) => centsToApi(sumCents(amounts) ?? 0n);
  return {
    received: sum(periods.map((period) => period.received)),
    paid: sum(periods.map((period) => period.paid)),
  };
}
