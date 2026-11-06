import { useEffect, useMemo, useRef, useState } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type BankAccount, type CashFlow, type CashFlowFilters, type CashFlowPeriod } from "../api";
import { ChartCard } from "../components/charts/ChartCard";
import { Legend, type LegendItem } from "../components/charts/Legend";
import { LineChart } from "../components/charts/LineChart";
import { StatTile } from "../components/charts/StatTile";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { defaultSpan, periodLabel, totalsOf } from "../lib/cashFlow";
import { formatMoney } from "../lib/decimal";
import { formatDate, isIsoDate } from "../lib/entryInstallments";
import { readUpTo } from "../lib/readUpTo";
import { difference } from "../lib/signedAmount";
import { todayIso } from "../lib/settlement";

const REALIZED = "Saldo realizado";
const PROJECTED = "Saldo previsto";
const SERIES: LegendItem[] = [
  { name: REALIZED, color: "var(--series-1)" },
  { name: PROJECTED, color: "var(--chart-muted)" },
];

const negative = (amount: string) => (amount.startsWith("-") ? "num negative" : "num");

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: a reply still in flight for another company lands on an unmounted component.
export function CashFlowPage() {
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
    ? "Informe o primeiro e o último dia."
    : filters.from > filters.to
      ? "O primeiro dia não pode ser depois do último."
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
  const series = forecasting ? SERIES : SERIES.slice(0, 1);
  const labelOf = (index: number) => periodLabel(periods[index], flow?.granularity ?? "month");
  const unit = flow?.granularity === "day" ? "dia" : "mês";
  // The forecast counts what is open now, so a period that is over would be missing what was settled
  // after it ended: it is only shown from the period that has not ended yet (ADR 0032).
  const projectedOf = (period: CashFlowPeriod): string | null => (period.end < today ? null : period.projected_balance);

  return (
    <>
      <PageHeader
        title="Fluxo de caixa"
        description="O que entrou e saiu das contas, pelo dia em que aconteceu, e o que ainda está em aberto, pelo vencimento."
      />
      <section className="card filters-card">
        <div className="filters" role="search">
          <label>
            De
            <input
              type="date"
              value={filters.from}
              onChange={(event) => setFilters({ ...filters, from: event.target.value })}
            />
          </label>
          <label>
            Até
            <input
              type="date"
              value={filters.to}
              onChange={(event) => setFilters({ ...filters, to: event.target.value })}
            />
          </label>
          <label>
            Agrupar por
            <select
              value={filters.granularity}
              onChange={(event) =>
                setFilters({ ...filters, granularity: event.target.value as CashFlowFilters["granularity"] })
              }
            >
              <option value="month">Mês</option>
              <option value="day">Dia</option>
            </select>
          </label>
          <label>
            Conta
            <select value={filters.account} onChange={(event) => setFilters({ ...filters, account: event.target.value })}>
              <option value="">Todas as contas</option>
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
            <Spinner /> Carregando…
          </p>
        )
      ) : (
        <>
          <div className="kpi-row">
            <StatTile label={`Saldo em ${formatDate(flow.from)}`} value={formatMoney(flow.opening_balance)} note="No começo do primeiro dia." />
            <StatTile label="Entradas realizadas" value={formatMoney(totals.received)} note="Baixas de contas a receber." />
            <StatTile label="Saídas realizadas" value={formatMoney(totals.paid)} note="Baixas de contas a pagar." />
            <StatTile
              label={`Saldo em ${formatDate(flow.to)}`}
              value={formatMoney(last.closing_balance)}
              note={
                forecasting && projectedOf(last) !== null
                  ? `Previsto: ${formatMoney(projectedOf(last))}`
                  : "Só o que já aconteceu."
              }
            />
          </div>
          {flow.past_due !== null && (flow.past_due.to_receive !== "0" || flow.past_due.to_pay !== "0") && (
            <Notice
              tone="warning"
              message={`Em aberto e vencido antes de ${formatDate(flow.from)}: ${formatMoney(flow.past_due.to_receive)} a receber e ${formatMoney(flow.past_due.to_pay)} a pagar. O saldo previsto já conta com isso.`}
            />
          )}
          <ChartCard
            title="Saldo ao longo do período"
            description={
              forecasting
                ? "O saldo realizado e o previsto, se tudo o que está em aberto for pago no vencimento."
                : "O saldo da conta ao fim de cada período. A previsão vale para todas as contas juntas."
            }
            legend={<Legend items={series} kind="line" />}
            table={{
              columns: ["Período", ...series.map((item) => item.name)],
              rows: periods.map((period, index) => [
                labelOf(index),
                formatMoney(period.closing_balance),
                ...(forecasting ? [formatMoney(projectedOf(period))] : []),
              ]),
            }}
          >
            <LineChart
              label="Saldo ao longo do período"
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
            <h2>Período a período</h2>
            {loading && (
              <p role="status" className="status">
                <Spinner /> Atualizando…
              </p>
            )}
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Período</th>
                    <th className="num">Entradas</th>
                    <th className="num">Saídas</th>
                    <th className="num">Transferências</th>
                    <th className="num">Saldo final</th>
                    {forecasting && (
                      <>
                        <th className="num">A receber</th>
                        <th className="num">A pagar</th>
                        <th className="num">Saldo previsto</th>
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
                      <td className="num" title="O que entrou menos o que saiu da conta">
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
