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
import { useT, type Translate } from "../i18n";

/** `1 a 30 dias`, `91 dias ou mais`. */
function rangeLabel(from: number, to: number | null, t: Translate): string {
  return to === null ? t("{from} days or more", { from }) : t("{from} to {to} days", { from, to });
}

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function DelinquencyPage() {
  const t = useT();
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
    names.get(id) ?? (targets.loaded ? t("Customer {id}", { id: id.slice(0, 8) }) : "…");

  return (
    <>
      <PageHeader
        title={t("Delinquency")}
        description={t("Who owes, how much and for how many days: overdue receivable installments that still have a balance.")}
      />
      <Notice message={error ?? targets.error} />
      <section className="card">
        <label>
          {t("Position on")}
          <input type="date" value={on} onChange={(event) => setOn(event.target.value)} />
        </label>
      </section>
      {report === null && error === null && isIsoDate(on) ? (
        <p role="status" className="status">
          <Spinner /> {t("Loading…")}
        </p>
      ) : (
        report !== null && (
          <>
            <section className="card" aria-label={t("By days overdue")}>
              <h2>{t("By days overdue")}</h2>
              <div className="tiles">
                <div className="tile">
                  <span className="muted">{t("Total overdue")}</span>
                  <strong className="num">{formatMoney(report.owed)}</strong>
                  <span className="muted">{t("{count} installments", { count: report.installments })}</span>
                </div>
                {report.buckets.map((bucket) => (
                  <div className="tile" key={bucket.from_days}>
                    <span className="muted">{rangeLabel(bucket.from_days, bucket.to_days, t)}</span>
                    <strong className="num">{formatMoney(bucket.owed)}</strong>
                    <span className="muted">{t("{count} installments", { count: bucket.installments })}</span>
                  </div>
                ))}
              </div>
            </section>
            <section className="card">
              <h2>{t("By customer")}</h2>
              {report.customers.length === 0 ? (
                <p className="muted">{t("Nothing overdue on this date.")}</p>
              ) : (
                <div className="table-wrap">
                  <table>
                    <thead>
                      <tr>
                        <th>{t("Customer")}</th>
                        <th className="num">{t("Past due")}</th>
                        <th className="num">{t("Installments")}</th>
                        <th>{t("Oldest installment")}</th>
                        {report.buckets.map((bucket) => (
                          <th key={bucket.from_days} className="num">
                            {rangeLabel(bucket.from_days, bucket.to_days, t)}
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {report.customers.map((customer) => (
                        <tr key={customer.party_id ?? "none"}>
                          <td>
                            {customer.party_id === null
                              ? t("No customer")
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
