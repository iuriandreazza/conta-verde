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
import { formatDecimal, formatMoney } from "../lib/decimal";
import { formatDate, isIsoDate } from "../lib/entryInstallments";
import { fitted, monthSpan } from "../lib/reports";
import { toQuantity } from "../lib/saleLines";
import { sumSigned } from "../lib/signedAmount";
import { todayIso } from "../lib/settlement";
import { signedQuantity, trimCost } from "../lib/stock";
import { HelpTitle } from "../components/HelpTitle";

type View = "valuation" | "movements";

const SERIES = [{ name: "Valor em estoque", color: "var(--series-1)" }];
const MAX_BARS = 8;

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function StockReportsPage() {
  const { companyId = "" } = useParams();
  const [view, setView] = useState<View>("valuation");

  return (
    <>
      <PageHeader
        title="Relatórios de estoque"
        description="O que o estoque vale em um dia e o que se moveu em um período, produto a produto, ao custo médio."
      />
      <div className="segmented" role="group" aria-label="Relatório de estoque">
        <button type="button" aria-pressed={view === "valuation"} onClick={() => setView("valuation")}>
          Valorização
        </button>
        <button type="button" aria-pressed={view === "movements"} onClick={() => setView("movements")}>
          Movimentações do período
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
  const [on, setOn] = useState(() => todayIso());
  // A year still being typed (0002) is not a day to ask about.
  const valid = isIsoDate(on) && on >= "1000-01-01";
  const { report, error, loading } = useReport<StockValuation>(valid ? () => api.stockValuation(companyId, on) : null, `${companyId}|${valid ? on : ""}`);

  return (
    <>
      <section className="card filters-card">
        <div className="filters" role="search">
          <label>
            Em
            <input type="date" value={on} onChange={(event) => setOn(event.target.value)} />
          </label>
        </div>
        <Notice message={valid ? null : "Informe o dia da valorização."} />
      </section>
      <Notice message={error} />
      {report === null || !valid ? (
        valid &&
        loading && (
          <p role="status" className="status">
            <Spinner /> Carregando…
          </p>
        )
      ) : (
        <ValuationReport report={report} refreshing={loading} />
      )}
    </>
  );
}

function ValuationReport({ report, refreshing }: { report: StockValuation; refreshing: boolean }) {
  const rows = report.rows;
  const withStock = rows.filter((row) => toQuantity(row.quantity) !== null).length;
  const label = (row: StockValuation["rows"][number]) => (row.sku === null ? row.name : `${row.name} (${row.sku})`);

  return (
    <>
      {refreshing && (
        <p role="status" className="status muted">
          <Spinner size={16} /> Atualizando…
        </p>
      )}
      <div className="kpi-row">
        <StatTile label="Valor do estoque" value={formatMoney(report.total)} note={`Em ${formatDate(report.on)}, ao custo médio.`} />
        <StatTile label="Produtos com saldo" value={String(withStock)} note="Com quantidade acima de zero nesse dia." />
      </div>
      {report.negative > 0 && (
        <Notice
          tone="warning"
          message={`${report.negative} ${report.negative === 1 ? "produto com saldo negativo" : "produtos com saldo negativo"}: o valor deles é zero e o total não mostra a falta. Acerte com um inventário.`}
        />
      )}
      {rows.length === 0 ? (
        <div className="card empty">
          <p role="status">Nenhum produto com saldo nesse dia.</p>
        </div>
      ) : (
        <>
          <ChartCard
            title="Maiores valores em estoque, por produto"
            description={`Os ${Math.min(MAX_BARS, rows.length)} maiores; a tabela abaixo tem todos.`}
            legend={<Legend items={SERIES} kind="bar" />}
            table={{
              columns: ["Produto", "Valor", "% do total"],
              rows: rows.map((row) => [label(row), formatMoney(row.value), `${formatDecimal(row.share, 2)}%`]),
            }}
          >
            <BarChart
              label="Maiores valores em estoque por produto"
              series={SERIES}
              groups={rows.slice(0, MAX_BARS).map((row) => ({ label: fitted(row.name), values: [row.value] }))}
            />
          </ChartCard>
          <section className="card list-card">
            <h2>Valorização por produto</h2>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Produto</th>
                    <th className="num">Quantidade</th>
                    <th className="num">Custo médio</th>
                    <th className="num">Valor</th>
                    <th className="num">% do total</th>
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
  const [today] = useState(() => todayIso());
  const [span, setSpan] = useState(() => monthSpan(today));
  const spanError =
    !isIsoDate(span.from) || !isIsoDate(span.to)
      ? "Informe o primeiro e o último dia."
      : span.from > span.to
        ? "O primeiro dia não pode ser depois do último."
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
          {report.items.length === 0 ? (
            <div className="card empty">
              <p role="status">Nenhum produto se moveu nesse período.</p>
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
  const rows = report.items;
  return (
    <section className="card list-card">
      <HelpTitle level={2} title={`Movimentações de ${formatDate(report.from)} a ${formatDate(report.to)}`}>
        <p>Saldo inicial mais entradas, menos saídas, mais ajustes, é o saldo final. Os ajustes têm sinal.</p>
      </HelpTitle>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Produto</th>
              <th className="num">Saldo inicial</th>
              <th className="num">Entradas</th>
              <th className="num">Saídas</th>
              <th className="num">Ajustes</th>
              <th className="num">Saldo final</th>
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
              <th>Total (valores)</th>
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
