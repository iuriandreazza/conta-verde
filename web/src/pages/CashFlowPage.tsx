import { useEffect, useMemo, useRef, useState } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type BankAccount, type CashFlow, type CashFlowFilters, type CashFlowPeriod } from "../api";
import { ChartCard } from "../components/charts/ChartCard";
import { Legend, type LegendItem } from "../components/charts/Legend";
import { LineChart } from "../components/charts/LineChart";
import { StatTile } from "../components/charts/StatTile";
import { Notice } from "../components/Notice";
import { PeriodFilter } from "../components/PeriodFilter";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { defaultSpan, periodLabel, totalsOf } from "../lib/cashFlow";
import { formatMoney } from "../lib/decimal";
import { formatDate, isIsoDate } from "../lib/entryInstallments";
import { readUpTo } from "../lib/readUpTo";
import { difference } from "../lib/signedAmount";
import { todayIso } from "../lib/settlement";
import { msg, useT } from "../i18n";

const REALIZED = msg("Realized balance");
const PROJECTED = msg("Projected balance");
const SERIES: LegendItem[] = [
  { name: REALIZED, color: "var(--series-1)" },
  { name: PROJECTED, color: "var(--chart-muted)", dashed: true },
];

const negative = (amount: string) => (amount.startsWith("-") ? "num negative" : "num");

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: a reply still in flight for another company lands on an unmounted component.
export function CashFlowPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const [filters, setFilters] = useState<CashFlowFilters>(() => ({
    ...defaultSpan(todayIso()),
    granularity: "month",
    account: "",
  }));
  const [accounts, setAccounts] = useState<BankAccount[]>([]);
  const [flow, setFlow] = useState<CashFlow | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [accountsError, setAccountsError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  // Today where the person is: the projected balance only means something from the period it falls in.
  const [today] = useState(() => todayIso());
  // Only the latest reply is used: a slow answer to an earlier span cannot replace a newer one.
  const latest = useRef(0);

  useEffect(() => {
    let current = true;
    readUpTo((after) => api.listBankAccounts(companyId, undefined, after))
      .then((list) => current && setAccounts(list))
      .catch((failure: unknown) => current && setAccountsError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  const spanError = !isIsoDate(filters.from) || !isIsoDate(filters.to)
    ? t("Give the first and the last day.")
    : filters.from > filters.to
      ? t("The first day cannot be after the last.")
      : null;

  useEffect(() => {
    if (spanError !== null) {
      return;
    }
    const read = ++latest.current;
    setLoading(true);
    api
      .getCashFlow(companyId, filters)
      .then((result) => {
        if (read === latest.current) {
          setFlow(result);
          setError(null);
        }
      })
      .catch((failure: unknown) => {
        if (read === latest.current) {
          setError(errorMessage(failure));
        }
      })
      .finally(() => {
        if (read === latest.current) {
          setLoading(false);
        }
      });
  }, [companyId, filters, spanError]);

  // What is drawn follows the reply that is on screen, not the filters being typed: after a refusal
  // the old periods stay, and they are still in the grouping and for the account they were asked.
  const periods = flow?.periods ?? [];
  const forecasting = periods.length > 0 && periods[0].projected_balance !== null;
  const totals = useMemo(() => totalsOf(periods), [periods]);
  const last = periods[periods.length - 1];
  const series = (forecasting ? SERIES : SERIES.slice(0, 1)).map((item) => ({ ...item, name: t(item.name) }));
  const labelOf = (index: number) => periodLabel(periods[index], flow?.granularity ?? "month");
  const unit = flow?.granularity === "day" ? t("day") : t("month");
  // The forecast counts what is open now, so a period that is over would be missing what was settled
  // after it ended: it is only shown from the period that has not ended yet (ADR 0032).
  const projectedOf = (period: CashFlowPeriod): string | null => (period.end < today ? null : period.projected_balance);

  return (
    <>
      <PageHeader
        title={t("Cash flow")}
        description={t("What came in and went out of the accounts, by the day it happened, and what is still open, by due date.")}
      />
      <section className="card filters-card">
        <PeriodFilter value={filters} onChange={(span) => setFilters({ ...filters, ...span })} today={today} />
        <div className="filters" role="search">
          <label>
            {t("Group by")}
            <select
              value={filters.granularity}
              onChange={(event) =>
                setFilters({ ...filters, granularity: event.target.value as CashFlowFilters["granularity"] })
              }
            >
              <option value="month">{t("Month")}</option>
              <option value="day">{t("Day")}</option>
            </select>
          </label>
          <label>
            {t("Account")}
            <select value={filters.account} onChange={(event) => setFilters({ ...filters, account: event.target.value })}>
              <option value="">{t("All accounts")}</option>
              {accounts.map((account) => (
                <option key={account.id} value={account.id}>
                  {account.name}
                </option>
              ))}
            </select>
          </label>
        </div>
        <Notice message={spanError} />
      </section>
      <Notice message={accountsError} />
      <Notice message={error} />
      {flow === null || spanError !== null ? (
        spanError === null &&
        loading && (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        )
      ) : (
        <>
          <div className="kpi-row">
            <StatTile label={t("Balance on {date}", { date: formatDate(flow.from) })} value={formatMoney(flow.opening_balance)} note={t("At the start of the first day.")} />
            <StatTile label={t("Realized inflows")} value={formatMoney(totals.received)} note={t("Settlements of receivables.")} />
            <StatTile label={t("Realized outflows")} value={formatMoney(totals.paid)} note={t("Settlements of payables.")} />
            <StatTile
              label={t("Balance on {date}", { date: formatDate(flow.to) })}
              value={formatMoney(last.closing_balance)}
              note={
                forecasting && projectedOf(last) !== null
                  ? t("Projected: {amount}", { amount: formatMoney(projectedOf(last)) })
                  : t("Only what has already happened.")
              }
            />
          </div>
          {flow.past_due !== null && (flow.past_due.to_receive !== "0" || flow.past_due.to_pay !== "0") && (
            <Notice
              tone="warning"
              message={t("Open and overdue before {date}: {receivable} to receive and {payable} to pay. The projected balance already counts on this.", {
                date: formatDate(flow.from),
                receivable: formatMoney(flow.past_due.to_receive),
                payable: formatMoney(flow.past_due.to_pay),
              })}
            />
          )}
          <ChartCard
            title={t("Balance over the period")}
            description={
              forecasting
                ? t("The realized and the projected balance, if everything that is open is paid on its due date.")
                : t("The account balance at the end of each period. The forecast applies to all accounts together.")
            }
            legend={<Legend items={series} kind="line" />}
            table={{
              columns: [t("Period"), ...series.map((item) => item.name)],
              rows: periods.map((period, index) => [
                labelOf(index),
                formatMoney(period.closing_balance),
                ...(forecasting ? [formatMoney(projectedOf(period))] : []),
              ]),
            }}
          >
            <LineChart
              label={t("Balance over the period")}
              unit={unit}
              series={series}
              points={periods.map((period, index) => ({
                label: labelOf(index),
                values: forecasting
                  ? [period.closing_balance, projectedOf(period) ?? period.closing_balance]
                  : [period.closing_balance],
              }))}
            />
          </ChartCard>
          <section className="card list-card">
            <h2>{t("Period by period")}</h2>
            {loading && (
              <p role="status" className="status">
                <Spinner /> {t("Updating…")}
              </p>
            )}
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>{t("Period")}</th>
                    <th className="num">{t("Inflows")}</th>
                    <th className="num">{t("Outflows")}</th>
                    <th className="num">{t("Transfers")}</th>
                    <th className="num">{t("Closing balance")}</th>
                    {forecasting && (
                      <>
                        <th className="num">{t("To receive")}</th>
                        <th className="num">{t("To pay")}</th>
                        <th className="num">{t("Projected balance")}</th>
                      </>
                    )}
                  </tr>
                </thead>
                <tbody>
                  {periods.map((period, index) => (
                    <tr key={period.start}>
                      <td className="tabular">{labelOf(index)}</td>
                      <td className="num">{formatMoney(period.received)}</td>
                      <td className="num">{formatMoney(period.paid)}</td>
                      <td className="num" title={t("What came in minus what went out of the account")}>
                        {flow.account_id === null ? "—" : formatMoney(difference(period.transfers_in, period.transfers_out))}
                      </td>
                      <td className={negative(period.closing_balance)}>{formatMoney(period.closing_balance)}</td>
                      {forecasting && (
                        <>
                          <td className="num">{formatMoney(period.to_receive)}</td>
                          <td className="num">{formatMoney(period.to_pay)}</td>
                          <td className={negative(projectedOf(period) ?? "0")}>{formatMoney(projectedOf(period))}</td>
                        </>
                      )}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        </>
      )}
    </>
  );
}
