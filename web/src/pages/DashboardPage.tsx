import { useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api, type CashFlow, type EntryKind, type OverdueReport } from "../api";
import { AlertGlyph } from "../components/charts/AlertGlyph";
import { BarChart } from "../components/charts/BarChart";
import { ChartCard } from "../components/charts/ChartCard";
import { Legend } from "../components/charts/Legend";
import { LineChart } from "../components/charts/LineChart";
import { StatTile } from "../components/charts/StatTile";
import { Button } from "../components/Button";
import { PlusIcon } from "../components/icons/plus";
import { RefreshCWIcon } from "../components/icons/refresh-cw";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { PeriodFilter } from "../components/PeriodFilter";
import { Spinner } from "../components/Spinner";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { dueBuckets, granularityOf, periodLabel, spanDays } from "../dashboard/figures";
import { CardBoundary } from "../dashboard/CardBoundary";
import { useLoaded, type Loaded } from "../dashboard/useLoaded";
import { formatMoney } from "../lib/decimal";
import { centsToApi, formatDate, isIsoDate, sumCents } from "../lib/entryInstallments";
import { PRESETS } from "../lib/period";
import { readUpTo } from "../lib/readUpTo";
import { todayIso } from "../lib/settlement";
import { addDays } from "../lib/termSplit";
import { msg, useI18n, useT, type Translate } from "../i18n";
import { useCanWrite } from "../lib/useCanWrite";

const FLOW_SERIES = [
  { name: msg("Inflows"), color: "var(--series-1)" },
  { name: msg("Outflows"), color: "var(--series-2)" },
];
const DUE_SERIES = [
  { name: msg("To receive"), color: "var(--series-1)" },
  { name: msg("To pay"), color: "var(--series-2)" },
];

/** The names of a chart's series are `msg` keys; the chart draws them as the person reads them. */
const named = (series: { name: string; color: string }[], t: Translate) => series.map((item) => ({ ...item, name: t(item.name) }));
const TOP = 5;
/** Names are read for the top rows of any company, so the cap is well above what a select offers. */
const MAX_NAMES = 5000;
/** The longest span the cash flow answers for by day (ADR 0032). */
const MAX_DAYS = 400;

const clock = new Intl.DateTimeFormat("pt-BR", { dateStyle: "short", timeStyle: "short" });

/** Shown in place of a card that could not be read, or while it is. */
function Waiting({ error }: { error: string | null }) {
  const t = useT();
  return error === null ? (
    <p role="status" className="status">
      <Spinner /> {t("Loading…")}
    </p>
  ) : (
    <Notice message={error} />
  );
}

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function DashboardPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  // Today where the person is: the server reads no clock, and every figure is "as of" it.
  const [today, setToday] = useState(() => todayIso());
  const [loadedAt, setLoadedAt] = useState(() => new Date());
  const [round, setRound] = useState(0);
  const [span, setSpan] = useState(() => PRESETS[0].span(today));
  const spanValid = isIsoDate(span.from) && isIsoDate(span.to) && span.from <= span.to && spanDays(span.from, span.to) <= MAX_DAYS;
  const reload = () => {
    // A page left open past midnight is about the new day after a refresh.
    setToday(todayIso());
    setLoadedAt(new Date());
    setRound((current) => current + 1);
  };
  // A new entry made from here or from the top bar changes every figure.
  useOnCreated("entry", reload);
  // One reading of the next 60 days feeds both receivable and payable blocks and the due chart.
  const forecast = useLoaded(() => api.getCashFlow(companyId, { from: today, to: addDays(today, 60), granularity: "day", account: "" }), [companyId, today, round]);

  return (
    <>
      <PageHeader
        title={t("Dashboard")}
        description={t("How the cash is, what falls due and what was sold.")}
        actions={
          <>
            <span className="muted">
              {t("Updated at {time}", { time: clock.format(loadedAt) })}
            </span>
            <Button type="button" variant="ghost" icon={RefreshCWIcon} onClick={reload}>
              {t("Refresh")}
            </Button>
          </>
        }
      />
      <CardBoundary key={round} title={t("Cash balance")}>
        <AccountsCard companyId={companyId} today={today} round={round} />
      </CardBoundary>
      <div className="chart-row">
        <CardBoundary key={round} title={t("To receive")}>
          <DueBlock companyId={companyId} today={today} round={round} forecast={forecast} kind="receivable" />
        </CardBoundary>
        <CardBoundary key={round} title={t("To pay")}>
          <DueBlock companyId={companyId} today={today} round={round} forecast={forecast} kind="payable" />
        </CardBoundary>
      </div>
      <CardBoundary key={round} title={t("Accounts by due date")}>
        <DueChart today={today} forecast={forecast} />
      </CardBoundary>
      <section className="card filters-card" aria-label={t("Period")}>
        <PeriodFilter value={span} onChange={setSpan} today={today} />
        {!spanValid && (
          <Notice message={t("Choose a period whose first day does not come after the last, of up to {days} days.", { days: MAX_DAYS })} />
        )}
      </section>
      {spanValid && (
        <>
          <CardBoundary key={round} title={t("Inflows and outflows")}>
            <FlowCard companyId={companyId} round={round} from={span.from} to={span.to} />
          </CardBoundary>
          <CardBoundary key={round} title={t("Sales in the period")}>
            <SalesCard companyId={companyId} round={round} from={span.from} to={span.to} />
          </CardBoundary>
        </>
      )}
    </>
  );
}

function AccountsCard({ companyId, today, round }: { companyId: string; today: string; round: number }) {
  const { t, rich } = useI18n();
  const balances = useLoaded(() => api.getBalances(companyId, today), [companyId, today, round]);
  const pending = useLoaded(() => api.listStatementLines(companyId), [companyId, round]);
  const toReconcile =
    pending.data === null ? null : `${pending.data.items.length}${pending.data.next_cursor === null ? "" : "+"}`;
  return (
    <section className="card hero" aria-label={t("Cash balance")}>
      {balances.data === null ? (
        <Waiting error={balances.error} />
      ) : (
        <>
          <div>
            <p className="stat-label">{t("Cash balance")}</p>
            <p className="hero-value">{formatMoney(balances.data.total)}</p>
            <p className="stat-note">{t("On {date}, adding up all accounts.", { date: formatDate(balances.data.on) })}</p>
          </div>
          <div>
            {balances.data.accounts.length === 0 ? (
              <p className="muted">
                {rich("No accounts yet. {link}", { link: <Link to={`/companies/${companyId}/bank-accounts`}>{t("Register an account")}</Link> })}
              </p>
            ) : (
              <ul className="plain-list" aria-label={t("Balance by account")}>
                {balances.data.accounts.map(({ account, balance }) => (
                  <li key={account.id}>
                    <span>{account.name}</span> <strong>{formatMoney(balance)}</strong>
                  </li>
                ))}
              </ul>
            )}
            {toReconcile !== null && toReconcile !== "0" && (
              <p>
                <Link to={`/companies/${companyId}/reconciliation`}>
                  {toReconcile === "1"
                    ? t("{count} statement line to reconcile", { count: toReconcile })
                    : t("{count} statement lines to reconcile", { count: toReconcile })}
                </Link>
              </p>
            )}
          </div>
        </>
      )}
    </section>
  );
}

const BLOCK = {
  receivable: { title: msg("To receive"), overdue: msg("Overdue receipts"), none: msg("Nothing to receive is overdue.") },
  payable: { title: msg("To pay"), overdue: msg("Overdue payments"), none: msg("Nothing to pay is overdue.") },
} as const;

/** One of the two mirrored blocks: what is overdue, what is due today and in the week, and the way to start one. */
function DueBlock({
  companyId,
  today,
  round,
  forecast,
  kind,
}: {
  companyId: string;
  today: string;
  round: number;
  forecast: Loaded<CashFlow>;
  kind: EntryKind;
}) {
  const t = useT();
  const canWrite = useCanWrite("finance");
  const { openCreate } = useCreation();
  const overdue =useLoaded(() => api.overdueReport(companyId, today, kind), [companyId, today, kind, round]);
  const words = BLOCK[kind];
  const buckets = forecast.data === null ? null : dueBuckets(forecast.data, today);
  const amount = (index: number) => (buckets === null ? null : buckets[index][kind]);
  return (
    <section className="card" aria-label={t(words.title)}>
      <div className="chart-head">
        <div>
          <h2>{t(words.title)}</h2>
          <p className="muted">{t("On {date}", { date: formatDate(today) })}</p>
        </div>
        {canWrite && (
          <Button type="button" variant="ghost" icon={PlusIcon} onClick={() => openCreate("entry", { kind })}>
            {kind === "receivable" ? t("New revenue") : t("New expense")}
          </Button>
        )}
      </div>
      {overdue.data === null ? (
        <Waiting error={overdue.error} />
      ) : (
        <StatTile
          tone={overdue.data.count > 0 ? "critical" : undefined}
          label={t(words.overdue)}
          value={formatMoney(kind === "receivable" ? overdue.data.receivable : overdue.data.payable)}
          note={
            overdue.data.count > 0 ? (
              <span className="delta delta-down">
                <AlertGlyph />{" "}
                {overdue.data.count === 1
                  ? t("{count} overdue item", { count: overdue.data.count })
                  : t("{count} overdue items", { count: overdue.data.count })}
              </span>
            ) : (
              t(words.none)
            )
          }
        />
      )}
      {buckets === null ? (
        <Waiting error={forecast.error} />
      ) : (
        <div className="kpi-row">
          <StatTile label={t("Due today")} value={formatMoney(amount(1))} />
          <StatTile label={t("Next 7 days")} value={formatMoney(amount(2))} />
          <StatTile label={t("8 to 30 days")} value={formatMoney(amount(3))} />
        </div>
      )}
      {overdue.data !== null && <OldestOverdue report={overdue.data} kind={kind} companyId={companyId} />}
    </section>
  );
}

function OldestOverdue({ report, kind, companyId }: { report: OverdueReport; kind: EntryKind; companyId: string }) {
  const t = useT();
  const oldest = report.items.filter((item) => item.kind === kind).slice(0, 3);
  if (oldest.length === 0) return null;
  return (
    <>
      <h3>{t("Oldest")}</h3>
      <ul className="plain-list" aria-label={kind === "receivable" ? t("Oldest overdue receivables") : t("Oldest overdue payables")}>
        {oldest.map((item) => (
          <li key={`${item.entry_id}-${item.installment}`}>
            {formatDate(item.due_date)} · {item.description} · <strong>{formatMoney(item.open)}</strong>
          </li>
        ))}
      </ul>
      <Link to={`/companies/${companyId}/reports`}>{t("See all overdue items")}</Link>
    </>
  );
}

function DueChart({ today, forecast }: { today: string; forecast: Loaded<CashFlow> }) {
  const t = useT();
  const buckets = useMemo(() => (forecast.data === null ? null : dueBuckets(forecast.data, today, t)), [forecast.data, today, t]);
  const series = named(DUE_SERIES, t);
  return (
    <ChartCard
      title={t("Accounts by due date")}
      description={t("What is still open, by due-date range, in the next 60 days")}
      legend={<Legend items={series} kind="bar" />}
      table={{
        columns: [t("Due date"), t("To receive"), t("To pay")],
        rows: (buckets ?? []).map((bucket) => [bucket.label, formatMoney(bucket.receivable), formatMoney(bucket.payable)]),
      }}
    >
      {buckets === null ? (
        <Waiting error={forecast.error} />
      ) : (
        <BarChart
          label={t("Receivables and payables by due-date range")}
          series={series}
          groups={buckets.map((bucket) => ({ label: bucket.label, alert: bucket.overdue, values: [bucket.receivable, bucket.payable] }))}
        />
      )}
    </ChartCard>
  );
}

function FlowCard({ companyId, round, from, to }: { companyId: string; round: number; from: string; to: string }) {
  const t = useT();
  const series = named(FLOW_SERIES, t);
  const granularity = granularityOf(from, to);
  const flow = useLoaded(() => api.getCashFlow(companyId, { from, to, granularity, account: "" }), [companyId, from, to, round]);
  const periods = flow.data?.periods ?? [];
  const result = useMemo(() => {
    const received = sumCents(periods.map((period) => period.received)) ?? 0n;
    const paid = sumCents(periods.map((period) => period.paid)) ?? 0n;
    return { received: centsToApi(received), paid: centsToApi(paid), net: centsToApi(received - paid) };
  }, [periods]);
  return (
    <ChartCard
      title={t("Inflows and outflows")}
      description={t("What was received and paid, from {from} to {to}", { from: formatDate(from), to: formatDate(to) })}
      legend={<Legend items={series} kind="line" />}
      table={{
        columns: [granularity === "month" ? t("Month") : t("Day"), t("Inflows"), t("Outflows")],
        rows: periods.map((period) => [periodLabel(period.start, granularity), formatMoney(period.received), formatMoney(period.paid)]),
      }}
    >
      {flow.data === null ? (
        <Waiting error={flow.error} />
      ) : (
        <>
          <p>
            <strong>{t("Result: {amount}", { amount: formatMoney(result.net) })}</strong>{" "}
            <span className="muted">
              ({t("{received} received, {paid} paid", { received: formatMoney(result.received), paid: formatMoney(result.paid) })})
            </span>
          </p>
          <LineChart
            label={t("Inflows and outflows in the period")}
            series={series}
            points={periods.map((period) => ({ label: periodLabel(period.start, granularity), values: [period.received, period.paid] }))}
          />
        </>
      )}
    </ChartCard>
  );
}

function SalesCard({ companyId, round, from, to }: { companyId: string; round: number; from: string; to: string }) {
  const t = useT();
  const products = useLoaded(() => api.salesReport(companyId, "product", from, to), [companyId, from, to, round]);
  const customers = useLoaded(() => api.salesReport(companyId, "customer", from, to), [companyId, from, to, round]);
  const names = useLoaded(
    async () => {
      const [parties, items] = await Promise.all([
        readUpTo((after) => api.listParties(companyId, undefined, after), MAX_NAMES),
        readUpTo((after) => api.listProducts(companyId, undefined, after), MAX_NAMES),
      ]);
      return new Map([...parties, ...items].map((item) => [item.id, item.name]));
    },
    [companyId, round],
  );
  const label = (key: string | null) => (key === null ? "—" : (names.data?.get(key) ?? "—"));
  return (
    <section className="card" aria-label={t("Sales in the period")}>
      <div className="chart-head">
        <div>
          <h2>{t("Sales in the period")}</h2>
          <p className="muted">
            {t("From {from} to {to}, less what was returned", { from: formatDate(from), to: formatDate(to) })}
          </p>
        </div>
        <Link to={`/companies/${companyId}/reports`}>{t("See the sales report")}</Link>
      </div>
      {products.data === null ? (
        <Waiting error={products.error} />
      ) : (
        <StatTile label={t("Total sold")} value={formatMoney(products.data.total)} note={products.data.sales === 1 ? t("{count} completed sale", { count: 1 }) : t("{count} completed sales", { count: products.data.sales })} />
      )}
      <div className="chart-row">
        <TopTable title={t("Best-selling products")} columns={t("Product or service")} report={products} label={label} />
        <TopTable title={t("Customers who bought the most")} columns={t("Customer")} report={customers} label={label} />
      </div>
    </section>
  );
}

function TopTable({
  title,
  columns,
  report,
  label,
}: {
  title: string;
  columns: string;
  report: { data: { rows: { key: string | null; net: string; share: string }[] } | null; error: string | null };
  label: (key: string | null) => string;
}) {
  const t = useT();
  return (
    <div>
      <h3>{title}</h3>
      {report.data === null ? (
        <Waiting error={report.error} />
      ) : report.data.rows.length === 0 ? (
        <p className="muted">{t("No completed sales in this period.")}</p>
      ) : (
        <div className="table-wrap">
          <table aria-label={title}>
            <thead>
              <tr>
                <th>{columns}</th>
                <th className="num">{t("Sold")}</th>
                <th className="num">{t("% of total")}</th>
              </tr>
            </thead>
            <tbody>
              {report.data.rows.slice(0, TOP).map((row) => (
                <tr key={row.key ?? "none"}>
                  <td>{label(row.key)}</td>
                  <td className="num">{formatMoney(row.net)}</td>
                  <td className="num">{row.share.replace(".", ",")}%</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
