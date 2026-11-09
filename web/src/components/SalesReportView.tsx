import { useEffect, useState } from "react";
import { api, errorMessage, type SalesGroup, type SalesReport } from "../api";
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

const SERIES = [{ name: "Vendido", color: "var(--series-1)" }];
const MAX_BARS = 8;

const GROUPS: { key: SalesGroup; label: string; none: string }[] = [
  { key: "customer", label: "Cliente", none: "—" },
  { key: "product", label: "Produto ou serviço", none: "—" },
  { key: "seller", label: "Vendedor", none: "Sem vendedor" },
];

const CLASS_HINT = "A: até 80% do total vendido; B: até 95%; C: o resto. Cada linha entra na classe em que começa.";

/** Names are read for every row a report can show, so the cap is well above what a select offers. */
const MAX_NAMES = 5000;

/**
 * Sales by customer, product or seller over a period, with each row's share and its place on the ABC curve
 * (ADR 0043). `names` holds the customers, sellers and products of the company by id.
 */
export function SalesReportView({ companyId, names: registers }: { companyId: string; names: Map<string, string> }) {
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
      ? "Informe o primeiro e o último dia."
      : span.from > span.to
        ? "O primeiro dia não pode ser depois do último."
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
  const label = (id: string | null) => (id === null ? group.none : (names.get(id) ?? "—"));
  const rows = report?.rows ?? [];

  return (
    <>
      <section className="card filters-card">
        <PeriodFilter value={span} onChange={setSpan} today={today} />
        <div className="filters" role="search">
          <label>
            Agrupar por
            <select value={by} onChange={(event) => setBy(event.target.value as SalesGroup)}>
              {GROUPS.map((item) => (
                <option key={item.key} value={item.key}>
                  {item.label}
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
            <Spinner /> Carregando…
          </p>
        )
      ) : (
        <>
          {loading && (
            <p role="status" className="status muted">
              <Spinner size={16} /> Atualizando…
            </p>
          )}
          <div className="kpi-row">
            <StatTile
              label="Total vendido"
              value={formatMoney(report.total)}
              note={`De ${formatDate(report.from)} a ${formatDate(report.to)}, menos o que foi devolvido.`}
            />
            <StatTile label="Vendas concluídas" value={String(report.sales)} note="Pedidos e vendas canceladas não entram." />
          </div>
          {rows.length === 0 ? (
            <div className="card empty">
              <p role="status">Nenhuma venda concluída nesse período.</p>
            </div>
          ) : (
            <>
              <ChartCard
                title={`Maiores valores, por ${group.label.toLowerCase()}`}
                description={`Os ${Math.min(MAX_BARS, rows.length)} maiores do período; a tabela abaixo tem todos.`}
                legend={<Legend items={SERIES} kind="bar" />}
                table={{
                  columns: [group.label, "Vendido", "% do total"],
                  rows: rows.map((row) => [label(row.key), formatMoney(row.net), `${formatDecimal(row.share, 2)}%`]),
                }}
              >
                <BarChart
                  label={`Maiores valores vendidos por ${group.label.toLowerCase()}`}
                  series={SERIES}
                  groups={rows.slice(0, MAX_BARS).map((row) => ({ label: fitted(label(row.key)), values: [row.net] }))}
                />
              </ChartCard>
              <section className="card list-card">
                <HelpTitle level={2} title={group.label}>
                  <p>Curva ABC. {CLASS_HINT}</p>
                </HelpTitle>
                <div className="table-wrap">
                  <table>
                    <thead>
                      <tr>
                        <th>{group.label}</th>
                        <th className="num">Vendas</th>
                        <th className="num">Vendido</th>
                        <th className="num">% do total</th>
                        <th className="num">% acumulado</th>
                        <th title={CLASS_HINT}>Classe</th>
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
