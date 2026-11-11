import { useEffect, useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import {
  api,
  errorMessage,
  type EntriesReport,
  type EntriesReportFilters,
  type EntryKind,
  type OverdueReport,
} from "../api";
import { BarChart } from "../components/charts/BarChart";
import { ChartCard } from "../components/charts/ChartCard";
import { Legend, type LegendItem } from "../components/charts/Legend";
import { StatTile } from "../components/charts/StatTile";
import { DreView } from "../components/DreView";
import { Notice } from "../components/Notice";
import { PeriodFilter } from "../components/PeriodFilter";
import { PageHeader } from "../components/PageHeader";
import { SalesReportView } from "../components/SalesReportView";
import { Spinner } from "../components/Spinner";
import { formatMoney } from "../lib/decimal";
import { centsToApi, formatDate, isIsoDate, sumCents } from "../lib/entryInstallments";
import { readUpTo } from "../lib/readUpTo";
import { fitted, monthSpan } from "../lib/reports";
import { difference } from "../lib/signedAmount";
import { todayIso } from "../lib/settlement";
import { msg, useT } from "../i18n";

const SERIES: LegendItem[] = [
  { name: msg("To receive"), color: "var(--series-1)" },
  { name: msg("To pay"), color: "var(--series-2)" },
];
/** The bars show the biggest groups; the table has all of them. */
const MAX_BARS = 8;
/** Names are read for every row a report can show, so the cap is well above what a select offers. */
const MAX_NAMES = 5000;

const GROUPS: { key: EntriesReportFilters["group"]; label: string; none: string }[] = [
  { key: "category", label: msg("Category"), none: msg("No category") },
  { key: "cost_center", label: msg("Cost center"), none: msg("No cost center") },
  { key: "party", label: msg("Customer or supplier"), none: msg("No customer or supplier") },
];

const negative = (amount: string) => (amount.startsWith("-") ? "num negative" : "num");

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function ReportsPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const [view, setView] = useState<"entries" | "overdue" | "sales" | "dre">("entries");
  const [names, setNames] = useState<Map<string, string>>(new Map());
  const [namesError, setNamesError] = useState<string | null>(null);

  // The reports answer in ids, and the registers they point at are read once for the names.
  useEffect(() => {
    let current = true;
    Promise.all([
      api.listCategories(companyId),
      readUpTo((after) => api.listCostCenters(companyId, { q: "", active: "", sort: "name" }, after), MAX_NAMES),
      readUpTo((after) => api.listParties(companyId, undefined, after), MAX_NAMES),
    ])
      .then(([categories, costCenters, parties]) => {
        if (!current) return;
        const byId = new Map<string, string>();
        for (const item of [...categories.items, ...costCenters, ...parties]) byId.set(item.id, item.name);
        setNames(byId);
      })
      .catch((failure: unknown) => current && setNamesError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  return (
    <>
      <PageHeader
        title={t("Reports")}
        description={t("Where the money goes, what is overdue, what was sold and the result of the period (income statement).")}
      />
      <div className="segmented" role="group" aria-label={t("Report")}>
        <button type="button" aria-pressed={view === "entries"} onClick={() => setView("entries")}>
          {t("Entries by group")}
        </button>
        <button type="button" aria-pressed={view === "overdue"} onClick={() => setView("overdue")}>
          {t("Past due")}
        </button>
        <button type="button" aria-pressed={view === "sales"} onClick={() => setView("sales")}>
          {t("Sales")}
        </button>
        <button type="button" aria-pressed={view === "dre"} onClick={() => setView("dre")}>
          {t("P&L")}
        </button>
      </div>
      <Notice message={namesError} />
      {view === "entries" ? (
        <EntriesReportView companyId={companyId} names={names} />
      ) : view === "overdue" ? (
        <OverdueView companyId={companyId} names={names} />
      ) : view === "sales" ? (
        <SalesReportView companyId={companyId} names={names} />
      ) : (
        <DreView companyId={companyId} />
      )}
    </>
  );
}

function EntriesReportView({ companyId, names }: { companyId: string; names: Map<string, string> }) {
  const t = useT();
  // Today where the person is: the usual periods are counted from it.
  const [today] = useState(() => todayIso());
  const [filters, setFilters] = useState<EntriesReportFilters>(() => ({
    group: "category",
    basis: "competence",
    ...monthSpan(today),
  }));
  const [report, setReport] = useState<EntriesReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const spanError = !isIsoDate(filters.from) || !isIsoDate(filters.to)
    ? t("Give the first and the last day.")
    : filters.from > filters.to
      ? t("The first day cannot be after the last.")
      : null;

  useEffect(() => {
    if (spanError !== null) return;
    setLoading(true);
    // A reply that arrives after the filters changed is not shown.
    let latest = true;
    api
      .entriesReport(companyId, filters)
      .then((result) => {
        if (latest) {
          setReport(result);
          setError(null);
        }
      })
      .catch((failure: unknown) => {
        if (latest) {
          setReport(null);
          setError(errorMessage(failure));
        }
      })
      .finally(() => latest && setLoading(false));
    return () => {
      latest = false;
    };
  }, [companyId, filters, spanError]);

  // What is drawn is the reply on screen, grouped as it was asked, not as the select says now.
  const group = GROUPS.find((item) => item.key === (report?.group ?? filters.group)) ?? GROUPS[0];
  const label = (id: string | null) => (id === null ? t(group.none) : (names.get(id) ?? "—"));
  const series = SERIES.map((item) => ({ ...item, name: t(item.name) }));
  const groupLabel = t(group.label);
  const rows = report?.rows ?? [];
  const totals = useMemo(() => {
    const receivable = centsToApi(sumCents(rows.map((row) => row.receivable)) ?? 0n);
    const payable = centsToApi(sumCents(rows.map((row) => row.payable)) ?? 0n);
    return { receivable, payable, net: difference(receivable, payable) };
  }, [rows]);

  return (
    <>
      <section className="card filters-card">
        <PeriodFilter value={filters} onChange={(span) => setFilters({ ...filters, ...span })} today={today} />
        <div className="filters" role="search">
          <label>
            {t("Group by")}
            <select
              value={filters.group}
              onChange={(event) => setFilters({ ...filters, group: event.target.value as EntriesReportFilters["group"] })}
            >
              {GROUPS.map((item) => (
                <option key={item.key} value={item.key}>
                  {t(item.label)}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Basis")}
            <select
              value={filters.basis}
              onChange={(event) => setFilters({ ...filters, basis: event.target.value as EntriesReportFilters["basis"] })}
            >
              <option value="competence">{t("Accrual date (the total of each entry)")}</option>
              <option value="due">{t("Due date (the installments that fall due)")}</option>
            </select>
          </label>
        </div>
        <Notice message={spanError} />
      </section>
      <Notice message={error} />
      {report === null || spanError !== null ? (
        spanError === null &&
        loading && (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        )
      ) : (
        <>
          <div className="kpi-row">
            <StatTile label={t("To receive")} value={formatMoney(totals.receivable)} note={t("From {from} to {to}.", { from: formatDate(report.from), to: formatDate(report.to) })} />
            <StatTile label={t("To pay")} value={formatMoney(totals.payable)} note={report.basis === "due" ? t("Installments due in the period.") : t("Total of the entries in the period.")} />
            <StatTile label={t("Result")} value={formatMoney(totals.net)} note={t("Receivable minus payable: what was recorded, not what was paid.")} />
          </div>
          {rows.length === 0 ? (
            <div className="card empty">
              <p role="status">{t("No entry in this period.")}</p>
            </div>
          ) : (
            <>
              <ChartCard
                title={t("Biggest groups, by {group}", { group: groupLabel.toLowerCase() })}
                description={t("The {count} biggest values of the period; the table below has all of them.", { count: Math.min(MAX_BARS, rows.length) })}
                legend={<Legend items={series} kind="bar" />}
                table={{
                  columns: [groupLabel, t("To receive"), t("To pay")],
                  rows: rows.map((row) => [label(row.id), formatMoney(row.receivable), formatMoney(row.payable)]),
                }}
              >
                <BarChart
                  label={t("Biggest groups by {group}", { group: groupLabel.toLowerCase() })}
                  series={series}
                  groups={rows.slice(0, MAX_BARS).map((row) => ({
                    label: fitted(label(row.id)),
                    values: [row.receivable, row.payable],
                  }))}
                />
              </ChartCard>
              <section className="card list-card">
                <h2>{groupLabel}</h2>
                <div className="table-wrap">
                  <table>
                    <thead>
                      <tr>
                        <th>{groupLabel}</th>
                        <th className="num">{t("To receive")}</th>
                        <th className="num">{t("To pay")}</th>
                        <th className="num">{t("Result")}</th>
                        <th className="num">{t("Entries")}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {rows.map((row) => (
                        <tr key={row.id ?? "none"}>
                          <td>{label(row.id)}</td>
                          <td className="num">{formatMoney(row.receivable)}</td>
                          <td className="num">{formatMoney(row.payable)}</td>
                          <td className={negative(row.net)}>{formatMoney(row.net)}</td>
                          <td className="num">{row.entries}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </section>
            </>
          )}
        </>
      )}
    </>
  );
}

function OverdueView({ companyId, names }: { companyId: string; names: Map<string, string> }) {
  const t = useT();
  const [today] = useState(() => todayIso());
  const [kind, setKind] = useState<"" | EntryKind>("");
  const [report, setReport] = useState<OverdueReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // The totals on screen belong to the answer they came with, not to the select that changed.
    setReport(null);
    setError(null);
    let latest = true;
    api
      .overdueReport(companyId, today, kind)
      .then((result) => {
        if (latest) {
          setReport(result);
          setError(null);
        }
      })
      .catch((failure: unknown) => latest && setError(errorMessage(failure)));
    return () => {
      latest = false;
    };
  }, [companyId, today, kind]);

  return (
    <>
      <section className="card filters-card">
        <div className="filters" role="search">
          <label>
            {t("Show")}
            <select value={kind} onChange={(event) => setKind(event.target.value as "" | EntryKind)}>
              <option value="">{t("Receivable and payable")}</option>
              <option value="receivable">{t("Receivables only")}</option>
              <option value="payable">{t("Payables only")}</option>
            </select>
          </label>
        </div>
      </section>
      <Notice message={error} />
      {report === null ? (
        error === null && (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        )
      ) : (
        <>
          <div className="kpi-row">
            <StatTile tone={report.receivable !== "0" ? "critical" : undefined} label={t("Overdue receivables")} value={formatMoney(report.receivable)} note={t("What customers still owe.")} />
            <StatTile label={t("Overdue payables")} value={formatMoney(report.payable)} note={t("What the company still owes.")} />
            <StatTile label={t("Overdue installments")} value={String(report.count)} note={t("Overdue before {date}.", { date: formatDate(report.on) })} />
          </div>
          {report.items.length === 0 ? (
            <div className="card empty">
              <p role="status">{t("Nothing overdue.")}</p>
            </div>
          ) : (
            <section className="card list-card">
              <h2>{t("Past due")}</h2>
              <div className="table-wrap">
                <table>
                  <thead>
                    <tr>
                      <th>{t("Due date")}</th>
                      <th>{t("Entry")}</th>
                      <th>{t("Customer or supplier")}</th>
                      <th className="num">{t("Open")}</th>
                      <th className="num">{t("Delay")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {report.items.map((item) => (
                      <tr key={`${item.entry_id}-${item.installment}`}>
                        <td className="tabular">{formatDate(item.due_date)}</td>
                        <td>
                          <div className="cell-title">{item.description}</div>
                          <div className="muted">
                            {t("{kind} · installment {number}", { kind: item.kind === "receivable" ? t("Receivable") : t("Payable"), number: item.installment })}
                          </div>
                        </td>
                        <td>{item.party_id === null ? "—" : (names.get(item.party_id) ?? "—")}</td>
                        <td className="num">{formatMoney(item.open)}</td>
                        <td className="num">{item.days_late === 1 ? t("1 day") : t("{count} days", { count: item.days_late })}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {report.count > report.items.length && (
                <p className="muted">
                  {t("Showing the {shown} oldest of {count}; the totals above are of all of them.", { shown: report.items.length, count: report.count })}
                </p>
              )}
            </section>
          )}
        </>
      )}
    </>
  );
}
