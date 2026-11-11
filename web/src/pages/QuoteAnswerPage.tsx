import { useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import { ApiError, api, errorMessage, type PublicQuote } from "../api";
import { Button } from "../components/Button";
import { CheckIcon } from "../components/icons/check";
import { FileTextIcon } from "../components/icons/file-text";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { useT } from "../i18n";
import { formatDecimal, formatMoney } from "../lib/decimal";
import { formatDate } from "../lib/entryInstallments";
import { QUOTE_STATUS_LABELS } from "../lib/quoteStatus";

type Loaded =
  | { status: "loading" }
  | { status: "ready"; quote: PublicQuote }
  | { status: "failed"; message: string };

/**
 * Where a quote's link leads: the customer sees the quote with no account and approves or declines it
 * once (ADR 0038). The token in the link is the only secret.
 */
export function QuoteAnswerPage() {
  const t = useT();
  const { token = "" } = useParams();
  const [loaded, setLoaded] = useState<Loaded>({ status: "loading" });
  const [answering, setAnswering] = useState<"approve" | "decline" | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    api
      .publicQuote(token)
      .then((quote) => current && setLoaded({ status: "ready", quote }))
      .catch((failure: unknown) => {
        if (current) {
          setLoaded({
            status: "failed",
            message:
              failure instanceof ApiError && failure.status === 404
                ? t("This link does not lead to any quote: it may have been replaced by a newer one. Ask whoever sent it for a new one.")
                : errorMessage(failure),
          });
        }
      });
    return () => {
      current = false;
    };
  }, [token]);

  async function answer(action: "approve" | "decline") {
    setAnswering(action);
    setError(null);
    try {
      const quote = await (action === "approve" ? api.approveQuote(token) : api.declineQuote(token));
      setLoaded({ status: "ready", quote });
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setAnswering(null);
    }
  }

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
        <PageHeader title={t("Quote")} />
        <Notice message={loaded.message} />
      </>
    );
  }
  const { quote } = loaded;
  const waiting = quote.status === "sent";
  return (
    <>
      <PageHeader
        title={t("Quote no. {number}", { number: quote.number })}
        subtitle={t("{company} for {customer}", { company: quote.company_name, customer: quote.customer_name })}
      />
      <section className="card">
        <p>
          {t("Issued on {issued} · valid until {valid}", { issued: formatDate(quote.issued_on), valid: formatDate(quote.valid_until) })}
          {quote.payment_term !== null && ` · ${t("payment: {term}", { term: quote.payment_term })}`}
        </p>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>{t("Item")}</th>
                <th className="num">{t("Quantity")}</th>
                <th className="num">{t("Price")}</th>
                <th className="num">{t("Discount")}</th>
                <th className="num">{t("Value")}</th>
              </tr>
            </thead>
            <tbody>
              {quote.lines.map((line, index) => (
                <tr key={index}>
                  <td>{line.description}</td>
                  <td className="num">{formatDecimal(line.quantity)}</td>
                  <td className="num">{formatMoney(line.unit_price)}</td>
                  <td className="num">{line.discount === "0" ? "—" : formatMoney(line.discount)}</td>
                  <td className="num">{formatMoney(line.total)}</td>
                </tr>
              ))}
            </tbody>
            <tfoot>
              <tr>
                <th colSpan={4}>{t("Total")}</th>
                <th className="num">{formatMoney(quote.total)}</th>
              </tr>
            </tfoot>
          </table>
        </div>
        {quote.notes !== null && <p className="muted">{quote.notes}</p>}
        <p>
          <a className="btn btn-ghost" href={api.publicQuotePdfUrl(token)} target="_blank" rel="noreferrer">
            <FileTextIcon size={16} aria-hidden /> {t("Download as PDF")}
          </a>
        </p>
        {waiting ? (
          <div className="form-actions">
            {quote.expired ? (
              <Notice tone="warning" message={t("This quote has expired: it can no longer be approved. Ask whoever sent it for a new one.")} />
            ) : (
              <Button icon={CheckIcon} pending={answering === "approve"} disabled={answering !== null} onClick={() => void answer("approve")}>
                {t("Approve quote")}
              </Button>
            )}
            <Button variant="ghost" pending={answering === "decline"} disabled={answering !== null} onClick={() => void answer("decline")}>
              {t("Decline")}
            </Button>
          </div>
        ) : (
          <Notice
            tone="success"
            message={
              quote.status === "approved" || quote.status === "converted"
                ? t("Quote approved. Thank you!")
                : t("Status: {status}.", { status: t(QUOTE_STATUS_LABELS[quote.status]) })
            }
          />
        )}
        <Notice message={error} />
      </section>
    </>
  );
}
