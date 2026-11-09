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
import { Notice } from "./Notice";
import { PeriodFilter } from "./PeriodFilter";
import { Spinner } from "./Spinner";

const SERIES: LegendItem[] = [
  { name: "Receita líquida", color: "var(--series-1)" },
  { name: "Custos e despesas", color: "var(--series-2)" },
];
const MAX_COST_CENTERS = 1000;
const negative = (amount: string) => (amount.startsWith("-") ? "num negative" : "num");

/**
 * The managerial income statement (ADR 0012, ADR 0070): by month or by cost center over a period, from the entries by
 * competence date, with the subtotals the server works out. It says it is managerial: it depends on how the entries were
 * classified.
 */
export function DreView({ companyId }: { companyId: string }) {
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
      ? "Informe o primeiro e o último dia."
      : span.from > span.to
        ? "O primeiro dia não pode ser depois do último."
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

  const names = new Map(centers.map((center) => [center.id, center.name]));
  // What is drawn is the reply on screen, with the columns it was asked for.
  const columnLabel = (key: string | null) =>
    dre?.by === "cost_center" ? (key === null ? "Sem centro de custo" : (names.get(key) ?? "—")) : monthColumnLabel(key ?? "");
  const columns = (dre?.columns ?? []).slice();
  if (dre?.by === "cost_center") {
    // By name, the entries that have none last.
    columns.sort((a, b) => (a.key === null ? 1 : b.key === null ? -1 : columnLabel(a.key).localeCompare(columnLabel(b.key), "pt-BR")));
  }

  function exportStatement() {
    if (dre === null) return;
    const header = ["Linha", ...columns.map((column) => columnLabel(column.key)), "Total"];
    const rows = DRE_ROWS.map((row) => [row.label.replace(/^= /, ""), ...columns.map((column) => column.lines[row.key]), dre.total[row.key]]);
    downloadCsv(`dre-${dre.from}-a-${dre.to}.csv`, toCsv(header, rows));
  }

  return (
    <>
      <section className="card filters-card">
        <PeriodFilter value={span} onChange={setSpan} today={today} />
        <div className="filters" role="search">
          <label>
            Colunas
            <select value={by} onChange={(event) => setBy(event.target.value as DreBy)}>
              <option value="month">Por mês</option>
              <option value="cost_center">Por centro de custo</option>
            </select>
          </label>
          <label>
            Centro de custo
            <select value={costCenter} onChange={(event) => setCostCenter(event.target.value)}>
              <option value="">Todos</option>
              <option value="none">Sem centro de custo</option>
              {centers.map((center) => (
                <option key={center.id} value={center.id}>
                  {center.name}
                </option>
              ))}
            </select>
          </label>
          <Button type="button" variant="ghost" disabled={dre === null} onClick={exportStatement}>
            Exportar
          </Button>
          <Button type="button" variant="ghost" onClick={() => window.print()}>
            Imprimir
          </Button>
        </div>
        <Notice message={spanError} />
      </section>
      <Notice message={error} />
      {dre === null || spanError !== null ? (
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
          {columns.length === 0 ? (
            <div className="card empty">
              <p role="status">Nenhum lançamento com competência nesse período.</p>
            </div>
          ) : (
            <>
          <ChartCard
            title="Receita líquida e custos e despesas"
            description="O resultado financeiro e o que fica abaixo da linha estão na tabela."
            legend={<Legend items={SERIES} kind="bar" />}
            table={{
              columns: ["Coluna", "Receita líquida", "Custos e despesas"],
              rows: columns.map((column) => [columnLabel(column.key), formatMoney(earning(column.lines)), formatMoney(spending(column.lines))]),
            }}
          >
            <BarChart
              label="Receita líquida e custos e despesas por coluna"
              series={SERIES}
              groups={columns.map((column) => ({
                label: fitted(columnLabel(column.key)),
                values: [earning(column.lines), spending(column.lines)],
              }))}
            />
          </ChartCard>
          <section className="card list-card">
            <h2>
              Demonstração do resultado de {formatDate(dre.from)} a {formatDate(dre.to)}
            </h2>
            <div className="table-wrap">
              <table>
                <caption className="visually-hidden">Demonstração do resultado</caption>
                <thead>
                  <tr>
                    <th scope="col">Linha</th>
                    {columns.map((column) => (
                      <th key={column.key ?? "none"} scope="col" className="num">
                        {columnLabel(column.key)}
                      </th>
                    ))}
                    <th scope="col" className="num">
                      Total
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {DRE_ROWS.map((row) => (
                    <tr key={row.key} className={row.subtotal ? "subtotal" : row.belowTheLine ? "below-the-line" : undefined}>
                      <th scope="row">{row.subtotal ? <strong>{row.label}</strong> : row.label}</th>
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
            <p className="muted">
              Demonstração gerencial: vem das datas de competência dos lançamentos e de como eles foram classificados. Não
              substitui a demonstração contábil. Investimentos e empréstimos ficam abaixo da linha, fora do resultado.
            </p>
          </section>
            </>
          )}
        </>
      )}
    </>
  );
}
