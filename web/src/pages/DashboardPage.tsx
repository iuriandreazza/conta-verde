import { useState } from "react";
import { AlertGlyph } from "../components/charts/AlertGlyph";
import { BarChart } from "../components/charts/BarChart";
import { ChartCard } from "../components/charts/ChartCard";
import { Legend } from "../components/charts/Legend";
import { LineChart } from "../components/charts/LineChart";
import { Sparkline, StatTile } from "../components/charts/StatTile";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { toDrawing } from "../lib/chart";
import { formatMoney } from "../lib/decimal";
import {
  BALANCE_TREND,
  CASH_BALANCE,
  DUE_BUCKETS,
  MONTHLY_FLOW,
  NEXT_30_DAYS,
  OVERDUE_RECEIVABLE,
  PERIODS,
  PERIOD_SUMMARY,
  UPCOMING,
  type PeriodMonths,
  type UpcomingItem,
} from "../dashboard/sampleData";

const FLOW_SERIES = [
  { name: "Entradas", color: "var(--series-1)" },
  { name: "Saídas", color: "var(--series-2)" },
];
const DUE_SERIES = [
  { name: "A receber", color: "var(--series-1)" },
  { name: "A pagar", color: "var(--series-2)" },
];

const STATUS: Record<UpcomingItem["status"], { label: string; className: string }> = {
  overdue: { label: "Vencida", className: "badge-critical" },
  today: { label: "Vence hoje", className: "badge-warning" },
  upcoming: { label: "Em dia", className: "badge-ok" },
};

/** The company's home. Every figure is sample data until phase 2 brings the financial module. */
export function DashboardPage() {
  const [months, setMonths] = useState<PeriodMonths>(6);

  const flow = MONTHLY_FLOW.slice(-months);
  const summary = PERIOD_SUMMARY[months];

  return (
    <>
      <PageHeader
        title="Painel"
        description="Como está o caixa e o que vence nos próximos dias."
        actions={
          <fieldset className="segmented">
            <legend className="visually-hidden">Período</legend>
            {PERIODS.map((period) => (
              <label key={period.months}>
                <input
                  type="radio"
                  name="period"
                  checked={months === period.months}
                  onChange={() => setMonths(period.months)}
                />
                <span>{period.label}</span>
              </label>
            ))}
          </fieldset>
        }
      />
      <Notice
        tone="warning"
        message="Dados de exemplo: o financeiro chega na próxima fase e estes números ainda não vêm do seu cadastro."
      />

      <section className="card hero" aria-label="Saldo em caixa">
        <div>
          <p className="stat-label">Saldo em caixa</p>
          <p className="hero-value">{formatMoney(CASH_BALANCE)}</p>
          <p className="stat-note">Fechamento de out/26</p>
        </div>
        <Sparkline values={BALANCE_TREND.map(toDrawing)} />
      </section>

      <div className="kpi-row">
        <StatTile label="A receber em 30 dias" value={formatMoney(NEXT_30_DAYS.receivable)} note="Vence até 05/12" />
        <StatTile label="A pagar em 30 dias" value={formatMoney(NEXT_30_DAYS.payable)} note="Vence até 05/12" />
        <StatTile
          label={`Resultado em ${PERIODS.find((period) => period.months === months)?.label}`}
          value={formatMoney(summary.result)}
          note={
            summary.change ? (
              <span className={`delta delta-${summary.change.direction}`}>
                {summary.change.direction === "up" ? "▲" : "▼"} {summary.change.text}
              </span>
            ) : (
              "Entradas menos saídas"
            )
          }
          trend={flow.map((month) => toDrawing(month.inflow) - toDrawing(month.outflow))}
        />
        <StatTile
          tone="critical"
          label="Recebimentos em atraso"
          value={formatMoney(OVERDUE_RECEIVABLE.amount)}
          note={
            <span className="delta delta-down">
              <AlertGlyph /> {OVERDUE_RECEIVABLE.count} títulos vencidos
            </span>
          }
        />
      </div>

      <div className="chart-row">
        <ChartCard
          title="Entradas e saídas"
          description={`Por mês, nos últimos ${months} meses`}
          legend={<Legend items={FLOW_SERIES} kind="line" />}
          table={{
            columns: ["Mês", "Entradas", "Saídas"],
            rows: flow.map((month) => [month.label, formatMoney(month.inflow), formatMoney(month.outflow)]),
          }}
        >
          <LineChart
            label="Entradas e saídas por mês"
            series={FLOW_SERIES}
            points={flow.map((month) => ({ label: month.label, values: [month.inflow, month.outflow] }))}
          />
        </ChartCard>

        <ChartCard
          title="Contas por vencimento"
          description="Quanto vence em cada faixa de prazo"
          legend={<Legend items={DUE_SERIES} kind="bar" />}
          table={{
            columns: ["Vencimento", "A receber", "A pagar"],
            rows: DUE_BUCKETS.map((bucket) => [bucket.label, formatMoney(bucket.receivable), formatMoney(bucket.payable)]),
          }}
        >
          <BarChart
            label="Contas a receber e a pagar por faixa de vencimento"
            series={DUE_SERIES}
            groups={DUE_BUCKETS.map((bucket) => ({
              label: bucket.label,
              alert: bucket.overdue,
              values: [bucket.receivable, bucket.payable],
            }))}
          />
        </ChartCard>
      </div>

      <section className="card">
        <div className="chart-head">
          <div>
            <h2>Próximos vencimentos</h2>
            <p className="muted">O que entra e o que sai nos próximos dias</p>
          </div>
        </div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Vence em</th>
                <th>Cliente ou fornecedor</th>
                <th>Tipo</th>
                <th>Situação</th>
                <th className="num">Valor</th>
              </tr>
            </thead>
            <tbody>
              {UPCOMING.map((item) => (
                <tr key={item.id}>
                  <td>{item.due}</td>
                  <td>
                    {item.party} <span className="muted sku">{item.description}</span>
                  </td>
                  <td>{item.direction === "receivable" ? "A receber" : "A pagar"}</td>
                  <td>
                    <span className={`badge ${STATUS[item.status].className}`}>
                      {item.status === "overdue" && <AlertGlyph size={12} />}
                      {STATUS[item.status].label}
                    </span>
                  </td>
                  <td className="num">{formatMoney(item.amount)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </>
  );
}
