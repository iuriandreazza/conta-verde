import { useEffect, useState } from "react";
import { api, errorMessage, type SalesGroup, type SalesReport } from "../api";
import { msg, useT } from "../i18n";
import { readUpTo } from "../lib/readUpTo";
import { formatDecimal, formatMoney } from "../lib/decimal";
import { formatDate, isIsoDate } from "../lib/entryInstallments";
import { fitted, monthSpan } from "../lib/reports";
import { todayIso } from "../lib/settlement";
import { BarChart } from "./charts/BarChart";
import { ChartCard } from "./charts/ChartCard";
import { Legend } from "./charts/Legend";
import { StatTile } from "./charts/StatTile";
import { Notice } from "./Notice";
import { PeriodFilter } from "./PeriodFilter";
import { Spinner } from "./Spinner";
import { HelpTitle } from "./HelpTitle";

const MAX_BARS = 8;

const GROUPS: { key: SalesGroup; label: string; none: string | null }[] = [
  { key: "customer", label: msg("Customer"), none: null },
  { key: "product", label: msg("Product or service"), none: null },
  { key: "seller", label: msg("Seller"), none: msg("No seller") },
];

const CLASS_HINT = msg("A: up to 80% of the total sold; B: up to 95%; C: the rest. Each row falls in the class where it starts.");

/** Names are read for every row a report can show, so the cap is well above what a select offers. */
const MAX_NAMES = 5000;

/**
 * Sales by customer, product or seller over a period, with each row's share and its place on the ABC curve
 * (ADR 0043). `names` holds the customers, sellers and products of the company by id.
 */
export function SalesReportView({ companyId, names: registers }: { companyId: string; names: Map<string, string> }) {
  const t = useT();
  const series = [{ name: t("Sold"), color: "var(--series-1)" }];
  const [productNames, setProductNames] = useState<Map<string, string>>(new Map());
  // A report that cannot name its products still shows their values: the names are a convenience.
  useEffect(() => {
    let current = true;
    readUpTo((after) => api.listProducts(companyId, undefined, after), MAX_NAMES)
      .then((products) => current && setProductNames(new Map(products.map((product) => [product.id, product.name]))))
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, [companyId]);
  const names = new Map([...registers, ...productNames]);
  const [today] = useState(() => todayIso());
  const [span, setSpan] = useState(() => monthSpan(today));
  const [by, setBy] = useState<SalesGroup>("customer");
  const [report, setReport] = useState<SalesReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const spanError =
    !isIsoDate(span.from) || !isIsoDate(span.to)
      ? t("Give the first and the last day.")
      : span.from > span.to
        ? t("The first day cannot be after the last.")
        : null;

  useEffect(() => {
    if (spanError !== null) return;
    setLoading(true);
    // A reply that arrives after the filters changed is not shown.
    let latest = true;
    api
      .salesReport(companyId, by, span.from, span.to)
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
  }, [companyId, by, span, spanError]);

  // What is drawn is the reply on screen, grouped as it was asked, not as the select says now.
  const group = GROUPS.find((item) => item.key === (report?.by ?? by)) ?? GROUPS[0];
  const groupLabel = t(group.label);
  const label = (id: string | null) => (id === null ? (group.none === null ? "—" : t(group.none)) : (names.get(id) ?? "—"));
  const rows = report?.rows ?? [];

  return (
    <>
      <section className="card filters-card">
        <PeriodFilter value={span} onChange={setSpan} today={today} />
        <div className="filters" role="search">
          <label>
            {t("Group by")}
            <select value={by} onChange={(event) => setBy(event.target.value as SalesGroup)}>
              {GROUPS.map((item) => (
                <option key={item.key} value={item.key}>
                  {t(item.label)}
                </option>
              ))}
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
          {loading && (
            <p role="status" className="status muted">
              <Spinner size={16} /> {t("Updating…")}
            </p>
          )}
          <div className="kpi-row">
            <StatTile
              label={t("Total sold")}
              value={formatMoney(report.total)}
              note={t("From {from} to {to}, less what was returned.", { from: formatDate(report.from), to: formatDate(report.to) })}
            />
            <StatTile label={t("Completed sales")} value={String(report.sales)} note={t("Orders and cancelled sales do not count.")} />
          </div>
          {rows.length === 0 ? (
            <div className="card empty">
              <p role="status">{t("No completed sales in this period.")}</p>
            </div>
          ) : (
            <>
              <ChartCard
                title={t("Largest values, by {group}", { group: groupLabel.toLowerCase() })}
                description={t("The {count} largest of the period; the table below has all of them.", { count: Math.min(MAX_BARS, rows.length) })}
                legend={<Legend items={series} kind="bar" />}
                table={{
                  columns: [groupLabel, t("Sold"), t("% of total")],
                  rows: rows.map((row) => [label(row.key), formatMoney(row.net), `${formatDecimal(row.share, 2)}%`]),
                }}
              >
                <BarChart
                  label={t("Largest values sold by {group}", { group: groupLabel.toLowerCase() })}
                  series={series}
                  groups={rows.slice(0, MAX_BARS).map((row) => ({ label: fitted(label(row.key)), values: [row.net] }))}
                />
              </ChartCard>
              <section className="card list-card">
                <HelpTitle level={2} title={groupLabel}>
                  <p>{t("ABC curve. {hint}", { hint: t(CLASS_HINT) })}</p>
                </HelpTitle>
                <div className="table-wrap">
                  <table>
                    <thead>
                      <tr>
                        <th>{groupLabel}</th>
                        <th className="num">{t("Sales")}</th>
                        <th className="num">{t("Sold")}</th>
                        <th className="num">{t("% of total")}</th>
                        <th className="num">{t("Cumulative %")}</th>
                        <th title={t(CLASS_HINT)}>{t("Class")}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {rows.map((row) => (
                        <tr key={row.key ?? "none"}>
                          <td>{label(row.key)}</td>
                          <td className="num">{row.sales}</td>
                          <td className="num">{formatMoney(row.net)}</td>
                          <td className="num">{formatDecimal(row.share, 2)}%</td>
                          <td className="num">{formatDecimal(row.cumulative, 2)}%</td>
                          <td>{row.class}</td>
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
