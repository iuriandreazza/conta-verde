import { useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type StockMovementSummary, type StockMovementSummaryRow, type StockValuation } from "../api";
import { BarChart } from "../components/charts/BarChart";
import { ChartCard } from "../components/charts/ChartCard";
import { Legend } from "../components/charts/Legend";
import { StatTile } from "../components/charts/StatTile";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { PeriodFilter } from "../components/PeriodFilter";
import { Spinner } from "../components/Spinner";
import { msg, useT } from "../i18n";
import { formatDecimal, formatMoney } from "../lib/decimal";
import { formatDate, isIsoDate } from "../lib/entryInstallments";
import { fitted, monthSpan } from "../lib/reports";
import { toQuantity } from "../lib/saleLines";
import { sumSigned } from "../lib/signedAmount";
import { todayIso } from "../lib/settlement";
import { signedQuantity, trimCost } from "../lib/stock";
import { HelpTitle } from "../components/HelpTitle";

type View = "valuation" | "movements";

const SERIES = [{ name: msg("Value in stock"), color: "var(--series-1)" }];
const MAX_BARS = 8;

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function StockReportsPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const [view, setView] = useState<View>("valuation");

  return (
    <>
      <PageHeader
        title={t("Stock reports")}
        description={t("What the stock is worth on a day and what moved in a period, product by product, at average cost.")}
      />
      <div className="segmented" role="group" aria-label={t("Stock report")}>
        <button type="button" aria-pressed={view === "valuation"} onClick={() => setView("valuation")}>
          {t("Valuation")}
        </button>
        <button type="button" aria-pressed={view === "movements"} onClick={() => setView("movements")}>
          {t("Movements of the period")}
        </button>
      </div>
      {view === "valuation" ? <ValuationView companyId={companyId} /> : <MovementsView companyId={companyId} />}
    </>
  );
}

/** Reads a report whenever its inputs change, dropping a reply that arrives after they changed again. */
function useReport<T>(load: (() => Promise<T>) | null, key: string) {
  const [report, setReport] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (load === null) return;
    setLoading(true);
    let latest = true;
    load()
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
    // `key` stands for what `load` closes over.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return { report, error, loading };
}

function ValuationView({ companyId }: { companyId: string }) {
  const t = useT();
  const [on, setOn] = useState(() => todayIso());
  // A year still being typed (0002) is not a day to ask about.
  const valid = isIsoDate(on) && on >= "1000-01-01";
  const { report, error, loading } = useReport<StockValuation>(valid ? () => api.stockValuation(companyId, on) : null, `${companyId}|${valid ? on : ""}`);

  return (
    <>
      <section className="card filters-card">
        <div className="filters" role="search">
          <label>
            {t("As of")}
            <input type="date" value={on} onChange={(event) => setOn(event.target.value)} />
          </label>
        </div>
        <Notice message={valid ? null : t("Enter the valuation day.")} />
      </section>
      <Notice message={error} />
      {report === null || !valid ? (
        valid &&
        loading && (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        )
      ) : (
        <ValuationReport report={report} refreshing={loading} />
      )}
    </>
  );
}

function ValuationReport({ report, refreshing }: { report: StockValuation; refreshing: boolean }) {
  const t = useT();
  const series = SERIES.map((item) => ({ ...item, name: t(item.name) }));
  const rows = report.rows;
  const withStock = rows.filter((row) => toQuantity(row.quantity) !== null).length;
  const label = (row: StockValuation["rows"][number]) => (row.sku === null ? row.name : `${row.name} (${row.sku})`);

  return (
    <>
      {refreshing && (
        <p role="status" className="status muted">
          <Spinner size={16} /> {t("Updating…")}
        </p>
      )}
      <div className="kpi-row">
        <StatTile label={t("Stock value")} value={formatMoney(report.total)} note={t("On {date}, at average cost.", { date: formatDate(report.on) })} />
        <StatTile label={t("Products with a balance")} value={String(withStock)} note={t("With a quantity above zero on that day.")} />
      </div>
      {report.negative > 0 && (
        <Notice
          tone="warning"
          message={
            report.negative === 1
              ? t("{count} product with a negative balance: its value is zero and the total does not show the shortfall. Fix it with an inventory count.", { count: report.negative })
              : t("{count} products with a negative balance: their value is zero and the total does not show the shortfall. Fix it with an inventory count.", { count: report.negative })
          }
        />
      )}
      {rows.length === 0 ? (
        <div className="card empty">
          <p role="status">{t("No product has a balance on that day.")}</p>
        </div>
      ) : (
        <>
          <ChartCard
            title={t("Largest stock values, by product")}
            description={t("The {count} largest; the table below has them all.", { count: Math.min(MAX_BARS, rows.length) })}
            legend={<Legend items={series} kind="bar" />}
            table={{
              columns: [t("Product"), t("Value"), t("% of total")],
              rows: rows.map((row) => [label(row), formatMoney(row.value), `${formatDecimal(row.share, 2)}%`]),
            }}
          >
            <BarChart
              label={t("Largest stock values by product")}
              series={series}
              groups={rows.slice(0, MAX_BARS).map((row) => ({ label: fitted(row.name), values: [row.value] }))}
            />
          </ChartCard>
          <section className="card list-card">
            <h2>{t("Valuation by product")}</h2>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>{t("Product")}</th>
                    <th className="num">{t("Quantity")}</th>
                    <th className="num">{t("Average cost")}</th>
                    <th className="num">{t("Value")}</th>
                    <th className="num">{t("% of total")}</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((row) => (
                    <tr key={row.product_id}>
                      <td>
                        <div className="cell-title">{row.name}</div>
                        {row.sku !== null && <div className="muted">{row.sku}</div>}
                      </td>
                      <td className="num">
                        {formatDecimal(row.quantity)} {row.unit}
                      </td>
                      <td className="num">{row.average_cost === null ? "—" : formatMoney(trimCost(row.average_cost))}</td>
                      <td className="num">{formatMoney(row.value)}</td>
                      <td className="num">{formatDecimal(row.share, 2)}%</td>
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

function MovementsView({ companyId }: { companyId: string }) {
  const t = useT();
  const [today] = useState(() => todayIso());
  const [span, setSpan] = useState(() => monthSpan(today));
  const spanError =
    !isIsoDate(span.from) || !isIsoDate(span.to)
      ? t("Enter the first and the last day.")
      : span.from > span.to
        ? t("The first day cannot be after the last.")
        : null;
  // The server refuses a span over 366 days and says so: its message is shown as it is.
  const { report, error, loading } = useReport<StockMovementSummary>(
    spanError === null ? () => api.stockMovementSummary(companyId, span.from, span.to) : null,
    `${companyId}|${spanError === null ? `${span.from}|${span.to}` : ""}`,
  );

  return (
    <>
      <section className="card filters-card">
        <PeriodFilter value={span} onChange={setSpan} today={today} />
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
          {report.items.length === 0 ? (
            <div className="card empty">
              <p role="status">{t("No product moved in this period.")}</p>
            </div>
          ) : (
            <MovementsTable report={report} />
          )}
        </>
      )}
    </>
  );
}

/** The values of a column added up exactly, as money; `—` when one of them is not an amount. */
function totalOf(rows: StockMovementSummaryRow[], pick: (row: StockMovementSummaryRow) => string): string {
  const sum = sumSigned(rows.map(pick));
  return sum === null ? "—" : formatMoney(sum);
}

function MovementsTable({ report }: { report: StockMovementSummary }) {
  const t = useT();
  const rows = report.items;
  return (
    <section className="card list-card">
      <HelpTitle level={2} title={t("Movements from {from} to {to}", { from: formatDate(report.from), to: formatDate(report.to) })}>
        <p>{t("Opening balance plus entries, minus exits, plus adjustments, is the closing balance. Adjustments carry a sign.")}</p>
      </HelpTitle>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>{t("Product")}</th>
              <th className="num">{t("Opening balance")}</th>
              <th className="num">{t("Stock entries")}</th>
              <th className="num">{t("Exits")}</th>
              <th className="num">{t("Adjustments")}</th>
              <th className="num">{t("Closing balance")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.product_id}>
                <td>
                  <div className="cell-title">{row.name}</div>
                  {row.sku !== null && <div className="muted">{row.sku}</div>}
                </td>
                <Cell quantity={row.opening_quantity} value={row.opening_value} unit={row.unit} />
                <Cell quantity={row.entered_quantity} value={row.entered_value} unit={row.unit} />
                <Cell quantity={row.exited_quantity} value={row.exited_value} unit={row.unit} />
                <Cell quantity={row.adjusted_quantity} value={row.adjusted_value} unit={row.unit} signed />
                <Cell quantity={row.closing_quantity} value={row.closing_value} unit={row.unit} />
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr>
              <th>{t("Total (values)")}</th>
              <td className="num">{totalOf(rows, (row) => row.opening_value)}</td>
              <td className="num">{totalOf(rows, (row) => row.entered_value)}</td>
              <td className="num">{totalOf(rows, (row) => row.exited_value)}</td>
              <td className="num">{totalOf(rows, (row) => row.adjusted_value)}</td>
              <td className="num">{totalOf(rows, (row) => row.closing_value)}</td>
            </tr>
          </tfoot>
        </table>
      </div>
    </section>
  );
}

function Cell({ quantity, value, unit, signed = false }: { quantity: string; value: string; unit: string; signed?: boolean }) {
  const shown = formatDecimal(quantity);
  return (
    <td className="num">
      <div>
        {signed ? signedQuantity(shown) : shown} {unit}
      </div>
      <div className="muted">{formatMoney(value)}</div>
    </td>
  );
}
