import { useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type Delinquency } from "../api";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { formatMoney } from "../lib/decimal";
import { formatDate, isIsoDate } from "../lib/entryInstallments";
import { todayIso } from "../lib/settlement";
import { useEntryTargets } from "../lib/useEntryTargets";

/** `1 a 30 dias`, `91 dias ou mais`. */
function rangeLabel(from: number, to: number | null): string {
  return to === null ? `${from} dias ou mais` : `${from} a ${to} dias`;
}

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function DelinquencyPage() {
  const { companyId = "" } = useParams();
  const targets = useEntryTargets(companyId);
  const [on, setOn] = useState(todayIso());
  const [report, setReport] = useState<Delinquency | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // What is shown is always for the day in the field: another day, or none, drops the old report.
    setReport(null);
    if (!isIsoDate(on)) {
      return;
    }
    let current = true;
    api
      .getDelinquency(companyId, on)
      .then((loaded) => {
        if (current) {
          setReport(loaded);
          setError(null);
        }
      })
      .catch((failure: unknown) => {
        if (current) {
          setReport(null);
          setError(errorMessage(failure));
        }
      });
    return () => {
      current = false;
    };
  }, [companyId, on]);

  // The names are read a page at a time (up to 200), so a customer may not be among them: it is then called by
  // what it is, never by a guess.
  const names = new Map(targets.parties.map((party) => [party.id, party.name]));
  const nameOf = (id: string) =>
    names.get(id) ?? (targets.loaded ? `Cliente ${id.slice(0, 8)}` : "…");

  return (
    <>
      <PageHeader
        title="Inadimplência"
        description="Quem deve, quanto e há quantos dias: parcelas a receber vencidas e ainda com saldo."
      />
      <Notice message={error ?? targets.error} />
      <section className="card">
        <label>
          Posição em
          <input type="date" value={on} onChange={(event) => setOn(event.target.value)} />
        </label>
      </section>
      {report === null && error === null && isIsoDate(on) ? (
        <p role="status" className="status">
          <Spinner /> Carregando…
        </p>
      ) : (
        report !== null && (
          <>
            <section className="card" aria-label="Por faixa de atraso">
              <h2>Por faixa de atraso</h2>
              <div className="tiles">
                <div className="tile">
                  <span className="muted">Total em atraso</span>
                  <strong className="num">{formatMoney(report.owed)}</strong>
                  <span className="muted">{report.installments} parcelas</span>
                </div>
                {report.buckets.map((bucket) => (
                  <div className="tile" key={bucket.from_days}>
                    <span className="muted">{rangeLabel(bucket.from_days, bucket.to_days)}</span>
                    <strong className="num">{formatMoney(bucket.owed)}</strong>
                    <span className="muted">{bucket.installments} parcelas</span>
                  </div>
                ))}
              </div>
            </section>
            <section className="card">
              <h2>Por cliente</h2>
              {report.customers.length === 0 ? (
                <p className="muted">Nada em atraso nesta data.</p>
              ) : (
                <div className="table-wrap">
                  <table>
                    <thead>
                      <tr>
                        <th>Cliente</th>
                        <th className="num">Em atraso</th>
                        <th className="num">Parcelas</th>
                        <th>Mais antiga</th>
                        {report.buckets.map((bucket) => (
                          <th key={bucket.from_days} className="num">
                            {rangeLabel(bucket.from_days, bucket.to_days)}
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {report.customers.map((customer) => (
                        <tr key={customer.party_id ?? "none"}>
                          <td>
                            {customer.party_id === null
                              ? "Sem cliente"
                              : nameOf(customer.party_id)}
                          </td>
                          <td className="num">{formatMoney(customer.owed)}</td>
                          <td className="num">{customer.installments}</td>
                          <td className="tabular">{formatDate(customer.oldest_due)}</td>
                          {customer.by_bucket.map((amount, index) => (
                            <td key={index} className="num">
                              {amount === "0" ? "—" : formatMoney(amount)}
                            </td>
                          ))}
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </section>
          </>
        )
      )}
    </>
  );
}
