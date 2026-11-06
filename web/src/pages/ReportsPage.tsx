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
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { formatMoney } from "../lib/decimal";
import { centsToApi, formatDate, isIsoDate, sumCents } from "../lib/entryInstallments";
import { readUpTo } from "../lib/readUpTo";
import { fitted, monthSpan } from "../lib/reports";
import { difference } from "../lib/signedAmount";
import { todayIso } from "../lib/settlement";

const SERIES: LegendItem[] = [
  { name: "A receber", color: "var(--series-1)" },
  { name: "A pagar", color: "var(--series-2)" },
];
/** The bars show the biggest groups; the table has all of them. */
const MAX_BARS = 8;

const GROUPS: { key: EntriesReportFilters["group"]; label: string; none: string }[] = [
  { key: "category", label: "Categoria", none: "Sem categoria" },
  { key: "cost_center", label: "Centro de custo", none: "Sem centro de custo" },
  { key: "party", label: "Cliente ou fornecedor", none: "Sem cliente ou fornecedor" },
];

const negative = (amount: string) => (amount.startsWith("-") ? "num negative" : "num");

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function ReportsPage() {
  const { companyId = "" } = useParams();
  const [view, setView] = useState<"entries" | "overdue">("entries");
  const [names, setNames] = useState<Map<string, string>>(new Map());
  const [namesError, setNamesError] = useState<string | null>(null);

  // The reports answer in ids, and the registers they point at are read once for the names.
  useEffect(() => {
    let current = true;
    Promise.all([
      api.listCategories(companyId),
      readUpTo((after) => api.listCostCenters(companyId, { q: "", active: "", sort: "name" }, after)),
      readUpTo((after) => api.listParties(companyId, undefined, after)),
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
        title="Relatórios"
        description="Para onde vai o dinheiro, por categoria, centro de custo e cliente ou fornecedor, e o que está vencido."
      />
      <div className="segmented" role="group" aria-label="Relatório">
        <button type="button" aria-pressed={view === "entries"} onClick={() => setView("entries")}>
          Lançamentos por grupo
        </button>
        <button type="button" aria-pressed={view === "overdue"} onClick={() => setView("overdue")}>
          Em atraso
        </button>
      </div>
      <Notice message={namesError} />
      {view === "entries" ? (
        <EntriesReportView companyId={companyId} names={names} />
      ) : (
        <OverdueView companyId={companyId} names={names} />
      )}
    </>
  );
}

function EntriesReportView({ companyId, names }: { companyId: string; names: Map<string, string> }) {
  const [filters, setFilters] = useState<EntriesReportFilters>(() => ({
    group: "category",
    basis: "competence",
    ...monthSpan(todayIso()),
  }));
  const [report, setReport] = useState<EntriesReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const spanError = !isIsoDate(filters.from) || !isIsoDate(filters.to)
    ? "Informe o primeiro e o último dia."
    : filters.from > filters.to
      ? "O primeiro dia não pode ser depois do último."
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
      .catch((failure: unknown) => latest && setError(errorMessage(failure)))
      .finally(() => latest && setLoading(false));
    return () => {
      latest = false;
    };
  }, [companyId, filters, spanError]);

  // What is drawn is the reply on screen, grouped as it was asked, not as the select says now.
  const group = GROUPS.find((item) => item.key === (report?.group ?? filters.group)) ?? GROUPS[0];
  const label = (id: string | null) => (id === null ? group.none : (names.get(id) ?? "—"));
  const rows = report?.rows ?? [];
  const totals = useMemo(() => {
    const receivable = centsToApi(sumCents(rows.map((row) => row.receivable)) ?? 0n);
    const payable = centsToApi(sumCents(rows.map((row) => row.payable)) ?? 0n);
    return { receivable, payable, net: difference(receivable, payable) };
  }, [rows]);

  return (
    <>
      <section className="card filters-card">
        <div className="filters" role="search">
          <label>
            Agrupar por
            <select
              value={filters.group}
              onChange={(event) => setFilters({ ...filters, group: event.target.value as EntriesReportFilters["group"] })}
            >
              {GROUPS.map((item) => (
                <option key={item.key} value={item.key}>
                  {item.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Base
            <select
              value={filters.basis}
              onChange={(event) => setFilters({ ...filters, basis: event.target.value as EntriesReportFilters["basis"] })}
            >
              <option value="competence">Competência (o total de cada lançamento)</option>
              <option value="due">Vencimento (as parcelas que vencem)</option>
            </select>
          </label>
          <label>
            De
            <input type="date" value={filters.from} onChange={(event) => setFilters({ ...filters, from: event.target.value })} />
          </label>
          <label>
            Até
            <input type="date" value={filters.to} onChange={(event) => setFilters({ ...filters, to: event.target.value })} />
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
          <div className="kpi-row">
            <StatTile label="A receber" value={formatMoney(totals.receivable)} note={`De ${formatDate(report.from)} a ${formatDate(report.to)}.`} />
            <StatTile label="A pagar" value={formatMoney(totals.payable)} note={report.basis === "due" ? "Parcelas que vencem no período." : "Total dos lançamentos do período."} />
            <StatTile label="Resultado" value={formatMoney(totals.net)} note="A receber menos a pagar: o que foi lançado, não o que foi pago." />
          </div>
          {rows.length === 0 ? (
            <div className="card empty">
              <p role="status">Nenhum lançamento nesse período.</p>
            </div>
          ) : (
            <>
              <ChartCard
                title={`Maiores grupos, por ${group.label.toLowerCase()}`}
                description={`Os ${Math.min(MAX_BARS, rows.length)} maiores valores do período; a tabela abaixo tem todos.`}
                legend={<Legend items={SERIES} kind="bar" />}
                table={{
                  columns: [group.label, "A receber", "A pagar"],
                  rows: rows.map((row) => [label(row.id), formatMoney(row.receivable), formatMoney(row.payable)]),
                }}
              >
                <BarChart
                  label={`Maiores grupos por ${group.label.toLowerCase()}`}
                  series={SERIES}
                  groups={rows.slice(0, MAX_BARS).map((row) => ({
                    label: fitted(label(row.id)),
                    values: [row.receivable, row.payable],
                  }))}
                />
              </ChartCard>
              <section className="card list-card">
                <h2>{group.label}</h2>
                <div className="table-wrap">
                  <table>
                    <thead>
                      <tr>
                        <th>{group.label}</th>
                        <th className="num">A receber</th>
                        <th className="num">A pagar</th>
                        <th className="num">Resultado</th>
                        <th className="num">Lançamentos</th>
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
  const [today] = useState(() => todayIso());
  const [kind, setKind] = useState<"" | EntryKind>("");
  const [report, setReport] = useState<OverdueReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
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
            Mostrar
            <select value={kind} onChange={(event) => setKind(event.target.value as "" | EntryKind)}>
              <option value="">A receber e a pagar</option>
              <option value="receivable">Só a receber</option>
              <option value="payable">Só a pagar</option>
            </select>
          </label>
        </div>
      </section>
      <Notice message={error} />
      {report === null ? (
        error === null && (
          <p role="status" className="status">
            <Spinner /> Carregando…
          </p>
        )
      ) : (
        <>
          <div className="kpi-row">
            <StatTile tone={report.receivable !== "0" ? "critical" : undefined} label="A receber em atraso" value={formatMoney(report.receivable)} note="O que os clientes ainda devem." />
            <StatTile label="A pagar em atraso" value={formatMoney(report.payable)} note="O que a empresa ainda deve." />
            <StatTile label="Parcelas vencidas" value={String(report.count)} note={`Vencidas antes de ${formatDate(report.on)}.`} />
          </div>
          {report.items.length === 0 ? (
            <div className="card empty">
              <p role="status">Nada em atraso.</p>
            </div>
          ) : (
            <section className="card list-card">
              <h2>Em atraso</h2>
              <div className="table-wrap">
                <table>
                  <thead>
                    <tr>
                      <th>Vencimento</th>
                      <th>Lançamento</th>
                      <th>Cliente ou fornecedor</th>
                      <th className="num">Em aberto</th>
                      <th className="num">Atraso</th>
                    </tr>
                  </thead>
                  <tbody>
                    {report.items.map((item) => (
                      <tr key={`${item.entry_id}-${item.installment}`}>
                        <td className="tabular">{formatDate(item.due_date)}</td>
                        <td>
                          <div className="cell-title">{item.description}</div>
                          <div className="muted">
                            {item.kind === "receivable" ? "A receber" : "A pagar"} · parcela {item.installment}
                          </div>
                        </td>
                        <td>{item.party_id === null ? "—" : (names.get(item.party_id) ?? "—")}</td>
                        <td className="num">{formatMoney(item.open)}</td>
                        <td className="num">{item.days_late === 1 ? "1 dia" : `${item.days_late} dias`}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {report.count > report.items.length && (
                <p className="muted">
                  Mostrando as {report.items.length} mais antigas de {report.count}; os totais acima são de todas.
                </p>
              )}
            </section>
          )}
        </>
      )}
    </>
  );
}
