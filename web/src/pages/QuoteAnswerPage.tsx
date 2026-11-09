import { useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import { ApiError, api, errorMessage, type PublicQuote } from "../api";
import { Button } from "../components/Button";
import { CheckIcon } from "../components/icons/check";
import { FileTextIcon } from "../components/icons/file-text";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
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
                ? "Este link não leva a nenhum orçamento: pode ter sido trocado por outro mais novo. Peça um novo a quem enviou."
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
        <Spinner /> Carregando…
      </p>
    );
  }
  if (loaded.status === "failed") {
    return (
      <>
        <PageHeader title="Orçamento" />
        <Notice message={loaded.message} />
      </>
    );
  }
  const { quote } = loaded;
  const waiting = quote.status === "sent";
  return (
    <>
      <PageHeader
        title={`Orçamento nº ${quote.number}`}
        subtitle={`${quote.company_name} para ${quote.customer_name}`}
      />
      <section className="card">
        <p>
          Emitido em {formatDate(quote.issued_on)} · válido até {formatDate(quote.valid_until)}
          {quote.payment_term !== null && ` · pagamento: ${quote.payment_term}`}
        </p>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Item</th>
                <th className="num">Quantidade</th>
                <th className="num">Preço</th>
                <th className="num">Desconto</th>
                <th className="num">Valor</th>
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
                <th colSpan={4}>Total</th>
                <th className="num">{formatMoney(quote.total)}</th>
              </tr>
            </tfoot>
          </table>
        </div>
        {quote.notes !== null && <p className="muted">{quote.notes}</p>}
        <p>
          <a className="btn btn-ghost" href={api.publicQuotePdfUrl(token)} target="_blank" rel="noreferrer">
            <FileTextIcon size={16} aria-hidden /> Baixar em PDF
          </a>
        </p>
        {waiting ? (
          <div className="form-actions">
            {quote.expired ? (
              <Notice tone="warning" message="A validade deste orçamento acabou: ele já não pode ser aprovado. Peça um novo a quem enviou." />
            ) : (
              <Button icon={CheckIcon} pending={answering === "approve"} disabled={answering !== null} onClick={() => void answer("approve")}>
                Aprovar orçamento
              </Button>
            )}
            <Button variant="ghost" pending={answering === "decline"} disabled={answering !== null} onClick={() => void answer("decline")}>
              Recusar
            </Button>
          </div>
        ) : (
          <Notice
            tone="success"
            message={
              quote.status === "approved" || quote.status === "converted"
                ? "Orçamento aprovado. Obrigado!"
                : `Situação: ${QUOTE_STATUS_LABELS[quote.status]}.`
            }
          />
        )}
        <Notice message={error} />
      </section>
    </>
  );
}
