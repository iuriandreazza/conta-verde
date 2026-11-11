import { useEffect, useState } from "react";
import { api, errorMessage, type CostCenter, type Dre, type DreBy } from "../api";
import { downloadCsv, toCsv } from "../lib/csv";
import { formatMoney } from "../lib/decimal";
import { DRE_ROWS, earning, monthColumnLabel, spending } from "../lib/dre";
import { formatDate, isIsoDate } from "../lib/entryInstallments";
import { readUpTo } from "../lib/readUpTo";
import { fitted, monthSpan } from "../lib/reports";
import { todayIso } from "../lib/settlement";
import { BarChart } from "./charts/BarChart";
import { ChartCard } from "./charts/ChartCard";
import { Legend, type LegendItem } from "./charts/Legend";
import { Button } from "./Button";
import { InfoTip } from "./InfoTip";
import { Notice } from "./Notice";
import { PeriodFilter } from "./PeriodFilter";
import { Spinner } from "./Spinner";
import { msg, useT } from "../i18n";

const SERIES: LegendItem[] = [
  { name: msg("Net revenue"), color: "var(--series-1)" },
  { name: msg("Costs and expenses"), color: "var(--series-2)" },
];
const MAX_COST_CENTERS = 1000;
const negative = (amount: string) => (amount.startsWith("-") ? "num negative" : "num");

/**
 * The managerial income statement (ADR 0012, ADR 0070): by month or by cost center over a period, from the entries by
 * competence date, with the subtotals the server works out. It says it is managerial: it depends on how the entries were
 * classified.
 */
export function DreView({ companyId }: { companyId: string }) {
  const t = useT();
  const [today] = useState(() => todayIso());
  const [span, setSpan] = useState(() => monthSpan(today));
  const [by, setBy] = useState<DreBy>("month");
  const [costCenter, setCostCenter] = useState("");
  const [centers, setCenters] = useState<CostCenter[]>([]);
  const [dre, setDre] = useState<Dre | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const spanError =
    !isIsoDate(span.from) || !isIsoDate(span.to)
      ? t("Give the first and the last day.")
      : span.from > span.to
        ? t("The first day cannot be after the last.")
        : null;

  // A statement that cannot name its cost centers still shows their values.
  useEffect(() => {
    let current = true;
    readUpTo((after) => api.listCostCenters(companyId, { q: "", active: "", sort: "name" }, after), MAX_COST_CENTERS)
      .then((found) => current && setCenters(found))
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, [companyId]);

  useEffect(() => {
    if (spanError !== null) return;
    setLoading(true);
    // A reply that arrives after the filters changed is not shown.
    let latest = true;
    api
      .incomeStatement(companyId, span.from, span.to, by, costCenter)
      .then((result) => {
        if (latest) {
          setDre(result);
          setError(null);
        }
      })
      .catch((failure: unknown) => {
        if (latest) {
          setDre(null);
          setError(errorMessage(failure));
        }
      })
      .finally(() => latest && setLoading(false));
    return () => {
      latest = false;
    };
  }, [companyId, span, by, costCenter, spanError]);

  const series = SERIES.map((item) => ({ ...item, name: t(item.name) }));
  const names = new Map(centers.map((center) => [center.id, center.name]));
  // What is drawn is the reply on screen, with the columns it was asked for.
  const columnLabel = (key: string | null) =>
    dre?.by === "cost_center" ? (key === null ? t("No cost center") : (names.get(key) ?? "—")) : monthColumnLabel(key ?? "");
  const columns = (dre?.columns ?? []).slice();
  if (dre?.by === "cost_center") {
    // By name, the entries that have none last.
    columns.sort((a, b) => (a.key === null ? 1 : b.key === null ? -1 : columnLabel(a.key).localeCompare(columnLabel(b.key), "pt-BR")));
  }

  function exportStatement() {
    if (dre === null) return;
    const header = [t("Line"), ...columns.map((column) => columnLabel(column.key)), t("Total")];
    const rows = DRE_ROWS.map((row) => [t(row.label).replace(/^= /, ""), ...columns.map((column) => column.lines[row.key]), dre.total[row.key]]);
    downloadCsv(`dre-${dre.from}-a-${dre.to}.csv`, toCsv(header, rows));
  }

  return (
    <>
      <section className="card filters-card">
        <PeriodFilter value={span} onChange={setSpan} today={today} />
        <div className="filters" role="search">
          <label>
            {t("Columns")}
            <select value={by} onChange={(event) => setBy(event.target.value as DreBy)}>
              <option value="month">{t("By month")}</option>
              <option value="cost_center">{t("By cost center")}</option>
            </select>
          </label>
          <label>
            {t("Cost center")}
            <select value={costCenter} onChange={(event) => setCostCenter(event.target.value)}>
              <option value="">{t("Any")}</option>
              <option value="none">{t("No cost center")}</option>
              {centers.map((center) => (
                <option key={center.id} value={center.id}>
                  {center.name}
                </option>
              ))}
            </select>
          </label>
          <Button type="button" variant="ghost" disabled={dre === null} onClick={exportStatement}>
            {t("Export")}
          </Button>
          <Button type="button" variant="ghost" onClick={() => window.print()}>
            {t("Print")}
          </Button>
        </div>
        <Notice message={spanError} />
      </section>
      <Notice message={error} />
      {dre === null || spanError !== null ? (
        spanError === null &&
        loading && (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        )
      ) : (
        <>
          {loading && (
            <p role="status" className="status muted">
              <Spinner size={16} /> {t("Updating…")}
            </p>
          )}
          {columns.length === 0 ? (
            <div className="card empty">
              <p role="status">{t("No entry with an accrual date in this period.")}</p>
            </div>
          ) : (
            <>
          <ChartCard
            title={t("Net revenue and costs and expenses")}
            description={t("The financial result and what stays below the line are in the table.")}
            legend={<Legend items={series} kind="bar" />}
            table={{
              columns: [t("Column"), t("Net revenue"), t("Costs and expenses")],
              rows: columns.map((column) => [columnLabel(column.key), formatMoney(earning(column.lines)), formatMoney(spending(column.lines))]),
            }}
          >
            <BarChart
              label={t("Net revenue and costs and expenses by column")}
              series={series}
              groups={columns.map((column) => ({
                label: fitted(columnLabel(column.key)),
                values: [earning(column.lines), spending(column.lines)],
              }))}
            />
          </ChartCard>
          <section className="card list-card">
            <div className="card-title">
              <h2>
                {t("Income statement from {from} to {to}", { from: formatDate(dre.from), to: formatDate(dre.to) })}
              </h2>
              <InfoTip label={t("About this statement")}>
                <p>
                  {t("Managerial statement: it comes from the accrual dates of the entries and from how they were classified. It does not replace the accounting statement. Investments and loans stay below the line, outside the result.")}
                </p>
              </InfoTip>
            </div>
            <div className="table-wrap">
              <table className="income-statement">
                <caption className="visually-hidden">{t("Income statement")}</caption>
                <thead>
                  <tr>
                    <th scope="col">{t("Line")}</th>
                    {columns.map((column) => (
                      <th key={column.key ?? "none"} scope="col" className="num">
                        {columnLabel(column.key)}
                      </th>
                    ))}
                    <th scope="col" className="num">
                      {t("Total")}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {DRE_ROWS.map((row) => (
                    <tr key={row.key} className={row.subtotal ? "subtotal" : row.belowTheLine ? "below-the-line" : undefined}>
                      <th scope="row">{row.subtotal ? <strong>{t(row.label)}</strong> : t(row.label)}</th>
                      {columns.map((column) => (
                        <td key={column.key ?? "none"} className={negative(column.lines[row.key])}>
                          {formatMoney(column.lines[row.key])}
                        </td>
                      ))}
                      <td className={negative(dre.total[row.key])}>
                        <strong>{formatMoney(dre.total[row.key])}</strong>
                      </td>
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
