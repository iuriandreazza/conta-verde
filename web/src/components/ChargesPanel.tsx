import { useCallback, useEffect, useState, type FormEvent } from "react";
import { Link } from "react-router-dom";
import {
  api,
  errorMessage,
  type Charge,
  type ChargeMethod,
  type ChargeStatus,
  type Entry,
} from "../api";
import { formatMoney } from "../lib/decimal";
import { formatDate, isIsoDate } from "../lib/entryInstallments";
import { balance, balanceCents } from "../lib/settlement";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { Notice } from "./Notice";
import { Spinner } from "./Spinner";

const METHOD_LABELS: Record<ChargeMethod, string> = {
  boleto: "Boleto",
  pix: "Pix",
  card_link: "Link de cartão",
};

const STATUS_LABELS: Record<ChargeStatus, string> = {
  open: "Aberta",
  paid: "Paga",
  cancelled: "Cancelada",
  expired: "Expirada",
};

/** What the server says in English when it kept a payment without settling it, in words for the person. */
const NOTES: Record<string, string> = {
  "the installment was already settled": "a parcela já estava baixada.",
  "the entry was cancelled": "o lançamento foi cancelado.",
  "the account charges are paid into is not set": "a conta de recebimento não está configurada.",
  "the installment no longer exists": "a parcela não existe mais.",
};

const STATUS_BADGES: Record<ChargeStatus, string> = {
  open: "badge badge-dot badge-warning",
  paid: "badge badge-dot badge-ok",
  cancelled: "badge badge-dot badge-muted",
  expired: "badge badge-dot badge-critical",
};

interface ChargesPanelProps {
  companyId: string;
  entry: Entry;
  canWrite: boolean;
}

/**
 * The charges of a receivable (ADR 0045): what was asked of the payment provider for each installment,
 * what the customer needs to pay (the boleto's line, the Pix text, the card link), and the form to issue
 * another. A charge is for what is left of its installment, and only one is open at a time.
 */
export function ChargesPanel({ companyId, entry, canWrite }: ChargesPanelProps) {
  const [charges, setCharges] = useState<Charge[] | null>(null);
  const [provider, setProvider] = useState<string | null | undefined>(undefined);
  const [providerError, setProviderError] = useState<string | null>(null);
  // Whether the account charges are paid into is chosen: until it is, none can be issued.
  const [accountChosen, setAccountChosen] = useState<boolean | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [issuing, setIssuing] = useState(false);
  const [cancellingId, setCancellingId] = useState<string | null>(null);
  const [chosen, setChosen] = useState<number | null>(null);

  const load = useCallback(async () => {
    try {
      setCharges((await api.listCharges(companyId, entry.id)).items);
      setError(null);
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }, [companyId, entry.id]);

  useEffect(() => {
    void load();
  }, [load]);

  // The provider is the operator's setting: it does not change while the page is open.
  useEffect(() => {
    let current = true;
    api
      .getCollection(companyId)
      .then((collection) => current && setProvider(collection.provider))
      .catch((failure) => current && setProviderError(errorMessage(failure)));
    api
      .getCollectionSettings(companyId)
      .then((settings) => current && setAccountChosen(settings.bank_account_id !== null))
      .catch((failure) => current && setProviderError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  const open = new Set(
    (charges ?? []).filter((charge) => charge.status === "open").map((charge) => charge.installment),
  );
  const chargeable = entry.installments.filter(
    (installment) => balanceCents(installment) > 0n && !open.has(installment.number),
  );
  const installment = chargeable.some((item) => item.number === chosen)
    ? (chosen as number)
    : (chargeable[0]?.number ?? 1);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (issuing) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const dueOn = String(form.get("due_on") ?? "");
    if (dueOn !== "" && !isIsoDate(dueOn)) {
      setFormError("Informe o vencimento da cobrança como data.");
      return;
    }
    setIssuing(true);
    setFormError(null);
    setSaved(null);
    try {
      await api.issueCharge(companyId, entry.id, {
        installment,
        method: String(form.get("method")) as ChargeMethod,
        ...(dueOn === "" ? {} : { due_on: dueOn }),
      });
      await load();
      setSaved(`Cobrança emitida para a parcela ${installment}.`);
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setIssuing(false);
    }
  }

  async function cancel(charge: Charge) {
    if (cancellingId !== null) {
      return;
    }
    setCancellingId(charge.id);
    setError(null);
    setSaved(null);
    try {
      await api.cancelCharge(companyId, entry.id, charge.id);
      await load();
    } catch (failure) {
      // Another tab may have moved it already: read what is true now, then say what happened (the
      // read clears the message).
      await load();
      setError(errorMessage(failure));
    } finally {
      setCancellingId(null);
    }
  }

  const canIssue =
    canWrite && provider !== null && provider !== undefined && accountChosen === true && !entry.voided;

  return (
    <div className="charges">
      <Notice message={error ?? providerError} />
      <Notice message={saved} tone="success" />
      {provider === "fake" && (
        <p className="muted">
          Provedor de teste: as cobranças são de mentira e não podem ser pagas.
        </p>
      )}
      {charges === null && error === null ? (
        <p role="status" className="status">
          <Spinner /> Carregando…
        </p>
      ) : charges === null ? (
        <Button type="button" variant="ghost" onClick={() => void load()}>
          Tentar de novo
        </Button>
      ) : (
        <>
          {charges.length === 0 ? (
            <p className="muted">Nenhuma cobrança emitida.</p>
          ) : (
            <ul className="history">
              {charges.map((charge) => (
                <li key={charge.id} className={charge.status === "open" ? undefined : "muted"}>
                  Parcela {charge.installment} · {METHOD_LABELS[charge.method]} ·{" "}
                  {formatMoney(charge.amount)}, vence em {formatDate(charge.due_on)}{" "}
                  <span className={STATUS_BADGES[charge.status]}>{STATUS_LABELS[charge.status]}</span>
                  {charge.status === "open" && <PaymentDetails charge={charge} />}
                  {charge.paid_on !== null && charge.paid_amount !== null && charge.fee !== null && (
                    <span className="muted">
                      {" "}
                      Paga em {formatDate(charge.paid_on)}: {formatMoney(charge.paid_amount)}, taxa{" "}
                      {formatMoney(charge.fee)}.
                    </span>
                  )}
                  <Notice
                    tone="warning"
                    message={
                      charge.note === null
                        ? null
                        : `Pagamento recebido, mas a parcela não foi baixada: ${NOTES[charge.note] ?? charge.note}`
                    }
                  />
                  {canWrite && charge.status === "open" && (
                    <Button
                      type="button"
                      variant="ghost"
                      className="btn-small"
                      aria-label={`Cancelar a cobrança da parcela ${charge.installment}`}
                      pending={cancellingId === charge.id}
                      disabled={cancellingId !== null}
                      onClick={() => void cancel(charge)}
                    >
                      Cancelar
                    </Button>
                  )}
                </li>
              ))}
            </ul>
          )}
          {provider === null && (
            <p className="muted">
              Cobrança por boleto, Pix ou link de cartão não está configurada nesta instalação.
            </p>
          )}
          {provider !== null && provider !== undefined && accountChosen === false && (
            <p className="muted">
              Escolha antes a conta em que o dinheiro das cobranças cai, em{" "}
              <Link to={`/companies/${companyId}/collection`}>Cobrança</Link>.
            </p>
          )}
          {canIssue &&
            (chargeable.length === 0 ? (
              <p className="muted">Nenhuma parcela com saldo e sem cobrança aberta.</p>
            ) : (
              <form onSubmit={handleSubmit} className="settle-form">
                <h3>Emitir uma cobrança</h3>
                <Notice message={formError} />
                <label>
                  Parcela
                  <select
                    name="installment"
                    value={installment}
                    onChange={(event) => setChosen(Number(event.target.value))}
                  >
                    {chargeable.map((item) => (
                      <option key={item.number} value={item.number}>
                        {item.number} — vence em {formatDate(item.due_date)}, saldo{" "}
                        {formatMoney(balance(item))}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Forma
                  <select name="method" defaultValue="pix">
                    {(Object.keys(METHOD_LABELS) as ChargeMethod[]).map((method) => (
                      <option key={method} value={method}>
                        {METHOD_LABELS[method]}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Vencimento da cobrança
                  <input name="due_on" type="date" />
                </label>
                <Button type="submit" icon={CheckIcon} pending={issuing}>
                  Emitir cobrança
                </Button>
              </form>
            ))}
        </>
      )}
    </div>
  );
}

/** What the customer pays with: a line, a text to paste or a link, each with what it takes to use it. */
function PaymentDetails({ charge }: { charge: Charge }) {
  const [copied, setCopied] = useState(false);

  // "Copiado" goes back to "Copiar" after a moment, so a second copy gives feedback again.
  useEffect(() => {
    if (!copied) {
      return;
    }
    const timer = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(timer);
  }, [copied]);

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
    } catch {
      // No clipboard where this runs (or no permission): the text stays on the page to be selected.
      setCopied(false);
    }
  }

  const text = charge.digitable_line ?? charge.pix_payload;
  return (
    <div className="charge-details">
      {text !== null && (
        <>
          <code className="tabular">{text}</code>{" "}
          <Button
            type="button"
            variant="ghost"
            className="btn-small"
            aria-label={`Copiar ${charge.method === "boleto" ? "a linha digitável" : "o código Pix"}`}
            onClick={() => void copy(text)}
          >
            Copiar
          </Button>
          <span role="status" className="muted">
            {copied ? "Copiado." : ""}
          </span>
        </>
      )}
      {charge.payment_url !== null && (
        <a href={charge.payment_url} target="_blank" rel="noopener noreferrer">
          {charge.method === "boleto" ? "Ver o boleto" : "Abrir o link de pagamento"}
        </a>
      )}
    </div>
  );
}
