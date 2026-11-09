import { useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import { ApiError, api, errorMessage, type PublicCharge } from "../api";
import { CopyButton } from "../components/CopyButton";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
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
                ? "Este link não leva a nenhuma cobrança. Peça um novo a quem enviou."
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
        <Spinner /> Carregando…
      </p>
    );
  }
  if (loaded.status === "failed") {
    return (
      <>
        <PageHeader title="Pagamento" />
        <Notice message={loaded.message} />
      </>
    );
  }
  const { charge } = loaded;
  return (
    <>
      <PageHeader
        title={`Pagamento para ${charge.company_name}`}
        subtitle={`${charge.description}${charge.installments > 1 ? ` — parcela ${charge.installment} de ${charge.installments}` : ""}`}
      />
      <section className="card">
        <p>
          Valor: <strong>{formatMoney(charge.amount)}</strong> · vencimento {formatDate(charge.due_on)}
        </p>
        {charge.state === "paid" && (
          <Notice
            tone="success"
            message={`Pagamento confirmado${charge.paid_on === null ? "" : ` em ${formatDate(charge.paid_on)}`}. Obrigado!`}
          />
        )}
        {charge.state === "unavailable" && (
          <Notice
            tone="warning"
            message={`Esta cobrança não está mais disponível para pagamento. Fale com ${charge.company_name}.`}
          />
        )}
        {charge.state === "payable" && (
          <>
            {charge.pix_payload !== null && (
              <div className="charge-details">
                <h2>Pix copia e cola</h2>
                <code className="tabular">{charge.pix_payload}</code>
                <CopyButton text={charge.pix_payload} label="Copiar o código Pix" />
              </div>
            )}
            {charge.digitable_line !== null && (
              <div className="charge-details">
                <h2>Boleto</h2>
                <code className="tabular">{charge.digitable_line}</code>
                <CopyButton text={charge.digitable_line} label="Copiar a linha digitável" />
              </div>
            )}
            {charge.payment_url !== null && (
              <p>
                <a className="btn" href={charge.payment_url} target="_blank" rel="noopener noreferrer">
                  {charge.method === "boleto" ? "Abrir o boleto (PDF)" : "Pagar com cartão"}
                </a>
              </p>
            )}
          </>
        )}
      </section>
    </>
  );
}
