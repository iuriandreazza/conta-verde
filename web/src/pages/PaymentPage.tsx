import { useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import { ApiError, api, errorMessage, type PublicCharge } from "../api";
import { CopyButton } from "../components/CopyButton";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { useI18n } from "../i18n";
import { formatMoney } from "../lib/decimal";
import { formatDate } from "../lib/entryInstallments";

type Loaded =
  | { status: "loading" }
  | { status: "ready"; charge: PublicCharge }
  | { status: "failed"; message: string };

/**
 * Where a charge's link leads: the customer sees what is being charged and pays with a boleto, Pix or a card
 * link, with no account (ADR 0047). The token in the link is the only secret.
 */
export function PaymentPage() {
  const { t, rich } = useI18n();
  const { token = "" } = useParams();
  const [loaded, setLoaded] = useState<Loaded>({ status: "loading" });

  useEffect(() => {
    let current = true;
    api
      .publicCharge(token)
      .then((charge) => current && setLoaded({ status: "ready", charge }))
      .catch((failure: unknown) => {
        if (current) {
          setLoaded({
            status: "failed",
            message:
              failure instanceof ApiError && failure.status === 404
                ? t("This link does not lead to any charge. Ask whoever sent it for a new one.")
                : errorMessage(failure),
          });
        }
      });
    return () => {
      current = false;
    };
  }, [token]);

  if (loaded.status === "loading") {
    return (
      <p role="status" className="status">
        <Spinner /> {t("Loading…")}
      </p>
    );
  }
  if (loaded.status === "failed") {
    return (
      <>
        <PageHeader title={t("Payment")} />
        <Notice message={loaded.message} />
      </>
    );
  }
  const { charge } = loaded;
  return (
    <>
      <PageHeader
        title={t("Payment to {company}", { company: charge.company_name })}
        subtitle={`${charge.description}${charge.installments > 1 ? ` — ${t("installment {number} of {total}", { number: charge.installment, total: charge.installments })}` : ""}`}
      />
      <section className="card">
        <p>
          {rich("Amount: {amount} · due {date}", { amount: <strong>{formatMoney(charge.amount)}</strong>, date: formatDate(charge.due_on) })}
        </p>
        {charge.state === "paid" && (
          <Notice
            tone="success"
            message={charge.paid_on === null ? t("Payment confirmed. Thank you!") : t("Payment confirmed on {date}. Thank you!", { date: formatDate(charge.paid_on) })}
          />
        )}
        {charge.state === "unavailable" && (
          <Notice
            tone="warning"
            message={t("This charge is no longer available for payment. Talk to {company}.", { company: charge.company_name })}
          />
        )}
        {charge.state === "payable" && (
          <>
            {charge.pix_payload !== null && (
              <div className="charge-details">
                <h2>{t("Pix copy and paste")}</h2>
                <code className="tabular">{charge.pix_payload}</code>
                <CopyButton text={charge.pix_payload} label={t("Copy the Pix code")} />
              </div>
            )}
            {charge.digitable_line !== null && (
              <div className="charge-details">
                <h2>{t("Boleto")}</h2>
                <code className="tabular">{charge.digitable_line}</code>
                <CopyButton text={charge.digitable_line} label={t("Copy the digitable line")} />
              </div>
            )}
            {charge.payment_url !== null && (
              <p>
                <a className="btn" href={charge.payment_url} target="_blank" rel="noopener noreferrer">
                  {charge.method === "boleto" ? t("Open the boleto (PDF)") : t("Pay by card")}
                </a>
              </p>
            )}
          </>
        )}
      </section>
    </>
  );
}
