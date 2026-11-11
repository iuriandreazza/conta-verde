import type { CashFlow } from "../api";
import { DEFAULT_LANGUAGE, translate, type Translate } from "../i18n";
import { centsToApi, sumCents } from "../lib/entryInstallments";
import { addDays } from "../lib/termSplit";

export interface DueBucket {
  label: string;
  overdue: boolean;
  receivable: string;
  payable: string;
}

const total = (amounts: (string | null)[]): string =>
  centsToApi(sumCents(amounts.filter((amount): amount is string => amount !== null)) ?? 0n);

/**
 * What is still open, by how far its due day is from `today`: what fell due before (the cash flow's
 * `past_due`), today, the next week, up to 30 days and up to 60. `flow` is the daily cash flow from
 * `today` through 60 days after it, for every account. The labels are in the language of `t`, the default one unless given.
 */
export function dueBuckets(flow: CashFlow, today: string, t: Translate = (key) => translate(DEFAULT_LANGUAGE, key)): DueBucket[] {
  const week = addDays(today, 7);
  const month = addDays(today, 30);
  const within = (keep: (start: string) => boolean) => flow.periods.filter((period) => keep(period.start));
  const bucket = (label: string, overdue: boolean, periods: CashFlow["periods"]): DueBucket => ({
    label,
    overdue,
    receivable: total(periods.map((period) => period.to_receive)),
    payable: total(periods.map((period) => period.to_pay)),
  });
  return [
    {
      label: t("Overdue"),
      overdue: true,
      receivable: flow.past_due?.to_receive ?? "0",
      payable: flow.past_due?.to_pay ?? "0",
    },
    bucket(t("Today"), false, within((start) => start === today)),
    bucket(t("1 to 7 days"), false, within((start) => start > today && start <= week)),
    bucket(t("8 to 30 days"), false, within((start) => start > week && start <= month)),
    bucket(t("31 to 60 days"), false, within((start) => start > month)),
  ];
}

const MONTHS = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];

/** A period's name in a chart: the day (`06/11`) or the month (`nov/26`). */
export function periodLabel(start: string, granularity: "day" | "month"): string {
  const [year, month, day] = start.split("-");
  return granularity === "month" ? `${MONTHS[Number(month) - 1]}/${year.slice(2)}` : `${day}/${month}`;
}

const DAY = 86_400_000;

/** Days in a span, counting both ends. */
export function spanDays(from: string, to: string): number {
  return (Date.parse(`${to}T00:00:00Z`) - Date.parse(`${from}T00:00:00Z`)) / DAY + 1;
}

/** Month by month once the span is long enough that days would crowd the chart. */
export function granularityOf(from: string, to: string): "day" | "month" {
  return spanDays(from, to) > 92 ? "month" : "day";
}
