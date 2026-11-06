import { useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api, type EntryKind, type OverdueReport } from "../api";
import { AlertGlyph } from "../components/charts/AlertGlyph";
import { BarChart } from "../components/charts/BarChart";
import { ChartCard } from "../components/charts/ChartCard";
import { Legend } from "../components/charts/Legend";
import { LineChart } from "../components/charts/LineChart";
import { StatTile } from "../components/charts/StatTile";
import { Button } from "../components/Button";
import { PlusIcon } from "../components/icons/plus";
import { RefreshCWIcon } from "../components/icons/refresh-cw";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { PeriodFilter } from "../components/PeriodFilter";
import { Spinner } from "../components/Spinner";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { dueBuckets, granularityOf, periodLabel, spanDays } from "../dashboard/figures";
import { CardBoundary } from "../dashboard/CardBoundary";
import { useLoaded } from "../dashboard/useLoaded";
import { formatMoney } from "../lib/decimal";
import { centsToApi, formatDate, isIsoDate, sumCents } from "../lib/entryInstallments";
import { PRESETS } from "../lib/period";
import { readUpTo } from "../lib/readUpTo";
import { todayIso } from "../lib/settlement";
import { addDays } from "../lib/termSplit";
import { useCanWrite } from "../lib/useCanWrite";

const FLOW_SERIES = [
  { name: "Entradas", color: "var(--series-1)" },
  { name: "Saídas", color: "var(--series-2)" },
];
const DUE_SERIES = [
  { name: "A receber", color: "var(--series-1)" },
  { name: "A pagar", color: "var(--series-2)" },
];
const TOP = 5;
/** The longest span the cash flow answers for by day (ADR 0032). */
const MAX_DAYS = 400;

const clock = new Intl.DateTimeFormat("pt-BR", { dateStyle: "short", timeStyle: "short" });

/** Shown in place of a card that could not be read, or while it is. */
function Waiting({ error }: { error: string | null }) {
  return error === null ? (
    <p role="status" className="status">
      <Spinner /> Carregando…
    </p>
  ) : (
    <Notice message={error} />
  );
}

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function DashboardPage() {
  const { companyId = "" } = useParams();
  // Today where the person is: the server reads no clock, and every figure is "as of" it.
  const [today] = useState(() => todayIso());
  const [loadedAt, setLoadedAt] = useState(() => new Date());
  const [round, setRound] = useState(0);
  const [span, setSpan] = useState(() => PRESETS[0].span(today));
  const spanValid = isIsoDate(span.from) && isIsoDate(span.to) && span.from <= span.to && spanDays(span.from, span.to) <= MAX_DAYS;
  const reload = () => {
    setLoadedAt(new Date());
    setRound((current) => current + 1);
  };
  // A new entry made from here or from the top bar changes every figure.
  useOnCreated("entry", reload);

  return (
    <>
      <PageHeader
        title="Painel"
        description="Como está o caixa, o que vence e o que foi vendido."
        actions={
          <>
            <span className="muted" role="status">
              Atualizado em {clock.format(loadedAt)}
            </span>
            <Button type="button" variant="ghost" icon={RefreshCWIcon} onClick={reload}>
              Atualizar
            </Button>
          </>
        }
      />
      <CardBoundary title="Saldo em caixa">
        <AccountsCard companyId={companyId} today={today} round={round} />
      </CardBoundary>
      <div className="chart-row">
        <CardBoundary title="A receber">
          <DueBlock companyId={companyId} today={today} round={round} kind="receivable" />
        </CardBoundary>
        <CardBoundary title="A pagar">
          <DueBlock companyId={companyId} today={today} round={round} kind="payable" />
        </CardBoundary>
      </div>
      <CardBoundary title="Contas por vencimento">
        <DueChart companyId={companyId} today={today} round={round} />
      </CardBoundary>
      <section className="card filters-card" aria-label="Período">
        <PeriodFilter value={span} onChange={setSpan} today={today} />
        {!spanValid && (
          <Notice message={`Escolha um período em que o primeiro dia não venha depois do último, de até ${MAX_DAYS} dias.`} />
        )}
      </section>
      {spanValid && (
        <>
          <CardBoundary title="Entradas e saídas">
            <FlowCard companyId={companyId} round={round} from={span.from} to={span.to} />
          </CardBoundary>
          <CardBoundary title="Vendas do período">
            <SalesCard companyId={companyId} round={round} from={span.from} to={span.to} />
          </CardBoundary>
        </>
      )}
    </>
  );
}

function AccountsCard({ companyId, today, round }: { companyId: string; today: string; round: number }) {
  const balances = useLoaded(() => api.getBalances(companyId, today), [companyId, today, round]);
  const pending = useLoaded(() => api.listStatementLines(companyId), [companyId, round]);
  const toReconcile =
    pending.data === null ? null : `${pending.data.items.length}${pending.data.next_cursor === null ? "" : "+"}`;
  return (
    <section className="card hero" aria-label="Saldo em caixa">
      {balances.data === null ? (
        <Waiting error={balances.error} />
      ) : (
        <>
          <div>
            <p className="stat-label">Saldo em caixa</p>
            <p className="hero-value">{formatMoney(balances.data.total)}</p>
            <p className="stat-note">Em {formatDate(balances.data.on)}, somando todas as contas.</p>
          </div>
          <div>
            {balances.data.accounts.length === 0 ? (
              <p className="muted">
                Nenhuma conta ainda. <Link to={`/companies/${companyId}/bank-accounts`}>Cadastrar uma conta</Link>
              </p>
            ) : (
              <ul className="plain-list" aria-label="Saldo por conta">
                {balances.data.accounts.map(({ account, balance }) => (
                  <li key={account.id}>
                    <span>{account.name}</span> <strong>{formatMoney(balance)}</strong>
                  </li>
                ))}
              </ul>
            )}
            {toReconcile !== null && toReconcile !== "0" && (
              <p>
                <Link to={`/companies/${companyId}/reconciliation`}>
                  {toReconcile} {toReconcile === "1" ? "linha de extrato" : "linhas de extrato"} para conciliar
                </Link>
              </p>
            )}
          </div>
        </>
      )}
    </section>
  );
}

const BLOCK = {
  receivable: { title: "A receber", overdue: "Recebimentos em atraso", none: "Nada a receber em atraso." },
  payable: { title: "A pagar", overdue: "Pagamentos em atraso", none: "Nada a pagar em atraso." },
} as const;

/** One of the two mirrored blocks: what is overdue, what is due today and in the week, and the way to start one. */
function DueBlock({ companyId, today, round, kind }: { companyId: string; today: string; round: number; kind: EntryKind }) {
  const canWrite = useCanWrite("finance");
  const { openCreate } = useCreation();
  const overdue = useLoaded(() => api.overdueReport(companyId, today, kind), [companyId, today, kind, round]);
  const forecast = useLoaded(() => api.getCashFlow(companyId, { from: today, to: addDays(today, 60), granularity: "day", account: "" }), [companyId, today, round]);
  const words = BLOCK[kind];
  const buckets = forecast.data === null ? null : dueBuckets(forecast.data, today);
  const amount = (index: number) => (buckets === null ? null : buckets[index][kind]);
  return (
    <section className="card" aria-label={words.title}>
      <div className="chart-head">
        <div>
          <h2>{words.title}</h2>
          <p className="muted">Em {formatDate(today)}</p>
        </div>
        {canWrite && (
          <Button type="button" variant="ghost" icon={PlusIcon} onClick={() => openCreate("entry")}>
            Novo lançamento
          </Button>
        )}
      </div>
      {overdue.data === null ? (
        <Waiting error={overdue.error} />
      ) : (
        <StatTile
          tone={overdue.data.count > 0 ? "critical" : undefined}
          label={words.overdue}
          value={formatMoney(kind === "receivable" ? overdue.data.receivable : overdue.data.payable)}
          note={
            overdue.data.count > 0 ? (
              <span className="delta delta-down">
                <AlertGlyph /> {overdue.data.count} {overdue.data.count === 1 ? "título vencido" : "títulos vencidos"}
              </span>
            ) : (
              words.none
            )
          }
        />
      )}
      {buckets === null ? (
        <Waiting error={forecast.error} />
      ) : (
        <div className="kpi-row">
          <StatTile label="Vence hoje" value={formatMoney(amount(1))} />
          <StatTile label="Próximos 7 dias" value={formatMoney(amount(2))} />
          <StatTile label="8 a 30 dias" value={formatMoney(amount(3))} />
        </div>
      )}
      {overdue.data !== null && <OldestOverdue report={overdue.data} kind={kind} companyId={companyId} />}
    </section>
  );
}

function OldestOverdue({ report, kind, companyId }: { report: OverdueReport; kind: EntryKind; companyId: string }) {
  const oldest = report.items.filter((item) => item.kind === kind).slice(0, 3);
  if (oldest.length === 0) return null;
  return (
    <>
      <h3>Mais antigos</h3>
      <ul className="plain-list" aria-label={`Mais antigos ${kind === "receivable" ? "a receber" : "a pagar"} em atraso`}>
        {oldest.map((item) => (
          <li key={`${item.entry_id}-${item.installment}`}>
            {formatDate(item.due_date)} · {item.description} · <strong>{formatMoney(item.open)}</strong>
          </li>
        ))}
      </ul>
      <Link to={`/companies/${companyId}/reports`}>Ver todos os atrasos</Link>
    </>
  );
}

function DueChart({ companyId, today, round }: { companyId: string; today: string; round: number }) {
  const forecast = useLoaded(() => api.getCashFlow(companyId, { from: today, to: addDays(today, 60), granularity: "day", account: "" }), [companyId, today, round]);
  const buckets = useMemo(() => (forecast.data === null ? null : dueBuckets(forecast.data, today)), [forecast.data, today]);
  return (
    <ChartCard
      title="Contas por vencimento"
      description="O que ainda está em aberto, por faixa de prazo, nos próximos 60 dias"
      legend={<Legend items={DUE_SERIES} kind="bar" />}
      table={{
        columns: ["Vencimento", "A receber", "A pagar"],
        rows: (buckets ?? []).map((bucket) => [bucket.label, formatMoney(bucket.receivable), formatMoney(bucket.payable)]),
      }}
    >
      {buckets === null ? (
        <Waiting error={forecast.error} />
      ) : (
        <BarChart
          label="Contas a receber e a pagar por faixa de vencimento"
          series={DUE_SERIES}
          groups={buckets.map((bucket) => ({ label: bucket.label, alert: bucket.overdue, values: [bucket.receivable, bucket.payable] }))}
        />
      )}
    </ChartCard>
  );
}

function FlowCard({ companyId, round, from, to }: { companyId: string; round: number; from: string; to: string }) {
  const granularity = granularityOf(from, to);
  const flow = useLoaded(() => api.getCashFlow(companyId, { from, to, granularity, account: "" }), [companyId, from, to, round]);
  const periods = flow.data?.periods ?? [];
  const result = useMemo(() => {
    const received = sumCents(periods.map((period) => period.received)) ?? 0n;
    const paid = sumCents(periods.map((period) => period.paid)) ?? 0n;
    return { received: centsToApi(received), paid: centsToApi(paid), net: centsToApi(received - paid) };
  }, [periods]);
  return (
    <ChartCard
      title="Entradas e saídas"
      description={`O que foi recebido e pago, de ${formatDate(from)} a ${formatDate(to)}`}
      legend={<Legend items={FLOW_SERIES} kind="line" />}
      table={{
        columns: [granularity === "month" ? "Mês" : "Dia", "Entradas", "Saídas"],
        rows: periods.map((period) => [periodLabel(period.start, granularity), formatMoney(period.received), formatMoney(period.paid)]),
      }}
    >
      {flow.data === null ? (
        <Waiting error={flow.error} />
      ) : (
        <>
          <p>
            <strong>Resultado: {formatMoney(result.net)}</strong>{" "}
            <span className="muted">
              ({formatMoney(result.received)} recebidos, {formatMoney(result.paid)} pagos)
            </span>
          </p>
          <LineChart
            label="Entradas e saídas no período"
            series={FLOW_SERIES}
            points={periods.map((period) => ({ label: periodLabel(period.start, granularity), values: [period.received, period.paid] }))}
          />
        </>
      )}
    </ChartCard>
  );
}

function SalesCard({ companyId, round, from, to }: { companyId: string; round: number; from: string; to: string }) {
  const products = useLoaded(() => api.salesReport(companyId, "product", from, to), [companyId, from, to, round]);
  const customers = useLoaded(() => api.salesReport(companyId, "customer", from, to), [companyId, from, to, round]);
  const names = useLoaded(
    async () => {
      const [parties, items] = await Promise.all([
        readUpTo((after) => api.listParties(companyId, undefined, after)),
        readUpTo((after) => api.listProducts(companyId, undefined, after)),
      ]);
      return new Map([...parties, ...items].map((item) => [item.id, item.name]));
    },
    [companyId, round],
  );
  const label = (key: string | null) => (key === null ? "—" : (names.data?.get(key) ?? "—"));
  return (
    <section className="card" aria-label="Vendas do período">
      <div className="chart-head">
        <div>
          <h2>Vendas do período</h2>
          <p className="muted">
            De {formatDate(from)} a {formatDate(to)}, menos o que foi devolvido
          </p>
        </div>
        <Link to={`/companies/${companyId}/reports`}>Ver o relatório de vendas</Link>
      </div>
      {products.data === null ? (
        <Waiting error={products.error} />
      ) : (
        <StatTile label="Total vendido" value={formatMoney(products.data.total)} note={`${products.data.sales} ${products.data.sales === 1 ? "venda concluída" : "vendas concluídas"}`} />
      )}
      <div className="chart-row">
        <TopTable title="Produtos mais vendidos" columns="Produto ou serviço" report={products} label={label} />
        <TopTable title="Clientes que mais compraram" columns="Cliente" report={customers} label={label} />
      </div>
    </section>
  );
}

function TopTable({
  title,
  columns,
  report,
  label,
}: {
  title: string;
  columns: string;
  report: { data: { rows: { key: string | null; net: string; share: string }[] } | null; error: string | null };
  label: (key: string | null) => string;
}) {
  return (
    <div>
      <h3>{title}</h3>
      {report.data === null ? (
        <Waiting error={report.error} />
      ) : report.data.rows.length === 0 ? (
        <p className="muted">Nenhuma venda concluída nesse período.</p>
      ) : (
        <div className="table-wrap">
          <table aria-label={title}>
            <thead>
              <tr>
                <th>{columns}</th>
                <th className="num">Vendido</th>
                <th className="num">% do total</th>
              </tr>
            </thead>
            <tbody>
              {report.data.rows.slice(0, TOP).map((row) => (
                <tr key={row.key ?? "none"}>
                  <td>{label(row.key)}</td>
                  <td className="num">{formatMoney(row.net)}</td>
                  <td className="num">{row.share.replace(".", ",")}%</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
